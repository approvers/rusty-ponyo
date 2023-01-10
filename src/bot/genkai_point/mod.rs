pub(crate) mod model;
mod plot;

use {
    crate::bot::{
        genkai_point::model::{Session, UserStat},
        parse_command, ui, BotService, Context, Message, SendAttachment, SendMessage,
    },
    anyhow::{Context as _, Result},
    async_trait::async_trait,
    chrono::{DateTime, Duration, Utc},
    clap::ValueEnum,
    once_cell::sync::Lazy,
    std::{cmp::Ordering, collections::HashMap, fmt::Write},
    tokio::sync::Mutex,
};

const NAME: &str = "rusty_ponyo::bot::genkai_point";
const PREFIX: &str = "g!point";

ui! {
    /// VCに入っている時間帯から、そのユーザーの限界さを計算しポイント化します
    struct Ui {
        name: NAME,
        prefix: PREFIX,
        command: Command,
    }
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    /// ヘルプメッセージを出します
    Help,

    /// ユーザーの限界ポイント情報を出します
    Show {
        /// 表示するユーザーのID
        user_id: Option<u64>,
    },

    /// トップN人のVC時間の伸びをグラフにプロットします
    Graph {
        /// トップ何人分表示するか
        #[clap(default_value_t = 5)]
        n: u8,
    },

    /// ランキングを出します
    Ranking {
        /// ランキングを反転します
        #[clap(long, short)]
        invert: bool,

        #[clap(value_enum, default_value_t=RankingBy::Point)]
        by: RankingBy,
    },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum)]
enum RankingBy {
    Point,
    Duration,
    Efficiency,
}

#[async_trait]
pub(crate) trait GenkaiPointDatabase: Send + Sync {
    /// Creates a new unclosed session if not exists.
    /// If the user's last session was closed before within 5minutes from now, clear its "left_at" field.
    /// If an unclosed session exists, leaves it untouched.
    async fn create_new_session(
        &self,
        user_id: u64,
        joined_at: DateTime<Utc>,
    ) -> Result<CreateNewSessionResult>;
    async fn unclosed_session_exists(&self, user_id: u64) -> Result<bool>;
    async fn close_session(&self, user_id: u64, left_at: DateTime<Utc>) -> Result<()>;
    async fn get_users_all_sessions(&self, user_id: u64) -> Result<Vec<Session>>;
    async fn get_all_users_who_has_unclosed_session(&self) -> Result<Vec<u64>>;
    async fn get_all_sessions(&self) -> Result<Vec<Session>>;

    async fn get_all_users_stats(&self) -> Result<Vec<UserStat>> {
        let sessions = self.get_all_sessions().await?;

        let user_sessions = {
            let mut map = HashMap::new();
            for session in sessions {
                map.entry(session.user_id)
                    .or_insert_with(Vec::new)
                    .push(session);
            }
            map
        };

        user_sessions
            .into_iter()
            .flat_map(|(_, x)| UserStat::from_sessions(&x).transpose())
            .collect::<Result<_>>()
            .context("failed to calc userstat")
    }
}

#[derive(Debug)]
pub(crate) enum CreateNewSessionResult {
    CreatedNewSession,
    UnclosedSessionExists,
    SessionResumed,
}

pub(crate) struct GenkaiPointBot<D> {
    db: D,
    resume_msg_timeout: Mutex<DateTime<Utc>>,
}

// chrono::Duration::seconds is not const fn yet.
static RESUME_MSG_TIMEOUT: Lazy<Duration> = Lazy::new(|| Duration::seconds(10));

impl<D: GenkaiPointDatabase> GenkaiPointBot<D> {
    pub(crate) fn new(db: D) -> Self {
        Self {
            db,
            resume_msg_timeout: Mutex::new(Utc::now()),
        }
    }

    async fn ranking<C>(&self, ctx: &dyn Context, by: &str, comparator: C) -> Result<()>
    where
        C: Fn(&UserStat, &UserStat) -> Ordering,
    {
        let mut ranking = self
            .db
            .get_all_users_stats()
            .await
            .context("failed to fetch ranking")?;

        ranking.sort_unstable_by_key(|x| x.user_id);
        ranking.sort_by(comparator);

        let d = drop;

        let mut result = String::with_capacity(256);
        d(writeln!(result, "```"));
        d(writeln!(result, "sorted by {by}"));

        let iter = ranking.iter().rev().take(20).enumerate();

        for (index, stat) in iter {
            let username = ctx
                .get_user_name(stat.user_id)
                .await
                .context("failed to get username")?;

            d(writeln!(
                result,
                "#{:02} {:5}pt. {:>7.2}h {:>5.2}%限界 {}",
                index + 1,
                stat.genkai_point,
                (stat.total_vc_duration.num_seconds() as f64) / 3600.,
                stat.efficiency * 100.0,
                username
            ))
        }

        d(writeln!(result, "```"));

        ctx.send_text_message(&result)
            .await
            .context("failed to send message")?;
        Ok(())
    }

    async fn graph(&self, ctx: &dyn Context, n: u8) -> Result<()> {
        let n = n.clamp(1, 11);

        #[cfg(feature = "plot_plotters")]
        let plotter = plot::plotters::Plotters;

        #[cfg(feature = "plot_matplotlib")]
        let plotter = plot::plotters::Matplotlib;

        #[cfg(all(feature = "plot_plotters", feature = "plot_matplotlib"))]
        compile_error!(
            "You can't enable both of plot_plotters and plot_matplotlib feature at the same time."
        );

        #[cfg(not(any(feature = "plot_plotters", feature = "plot_matplotlib")))]
        compile_error!("You must enable discord_client or console_client feature.");

        let image = plot::plot(&self.db, ctx, plotter, n as _).await?;

        match image {
            Some(image) => {
                ctx.send_message(SendMessage {
                    content: "",
                    attachments: &[SendAttachment {
                        name: "graph.png",
                        data: &image,
                    }],
                })
                .await
            }

            None => {
                ctx.send_text_message("プロットに必要なだけのデータがありません。")
                    .await
            }
        }
        .context("failed to send message")
    }

    async fn show(&self, ctx: &dyn Context, user_id: u64) -> Result<()> {
        let username = match ctx.get_user_name(user_id).await {
            Ok(n) => n,
            Err(_) => {
                ctx.send_text_message("ユーザーが見つかりませんでした")
                    .await
                    .context("failed to send message")?;
                return Ok(());
            }
        };

        let sessions = self
            .db
            .get_users_all_sessions(user_id)
            .await
            .context("failed to get sessions")?;

        let stat = UserStat::from_sessions(&sessions).context("failed to get userstat")?;

        let msg = match stat {
            Some(stat) => {
                format!(
                    "```\n{name}\n  - 限界ポイント: {points}pt.\n  - 合計VC時間: {vc_hour:.2}h\n  - 限界効率: {efficiency:.2}%\n```",
                    name = username,
                    points = stat.genkai_point,
                    vc_hour = stat.total_vc_duration.num_minutes() as f64 / 60.0,
                    efficiency = stat.efficiency * 100.0,
                )
            }

            None => format!("{username}さんの限界ポイントに関する情報は見つかりませんでした",),
        };

        ctx.send_text_message(&msg)
            .await
            .context("failed to send message")?;

        Ok(())
    }
}

fn comparator<C>(
    mapper: impl Fn(&UserStat) -> C,
    invert: bool,
) -> impl Fn(&UserStat, &UserStat) -> Ordering
where
    C: std::cmp::PartialOrd,
{
    move |a, b| {
        let res = mapper(a).partial_cmp(&mapper(b)).unwrap();
        if invert {
            res.reverse()
        } else {
            res
        }
    }
}

#[async_trait]
impl<D: GenkaiPointDatabase> BotService for GenkaiPointBot<D> {
    fn name(&self) -> &'static str {
        NAME
    }

    async fn on_message(&self, msg: &dyn Message, ctx: &dyn Context) -> Result<()> {
        if !msg.content().starts_with(PREFIX) {
            return Ok(());
        }

        let Some(parsed) = parse_command::<Ui>(msg.content(), ctx).await?
            else { return Ok(()) };

        match parsed.command {
            // help command should be handled automatically by clap
            Command::Help => {}

            Command::Show { user_id } => {
                self.show(ctx, user_id.unwrap_or_else(|| msg.author().id()))
                    .await?;
            }

            Command::Graph { n } => {
                self.graph(ctx, n).await?;
            }

            #[rustfmt::skip]
            Command::Ranking {
                by,
                invert,
            } => {
                match by {
                    RankingBy::Point => {
                        self.ranking(ctx,
                            "genkai point",
                            comparator(|x| x.genkai_point, invert),
                        ).await
                    }
                    RankingBy::Duration => {
                        self.ranking(ctx,
                            "total vc duration",
                            comparator(|x| x.total_vc_duration, invert),
                        ).await
                    }
                    RankingBy::Efficiency => {
                        self.ranking(ctx,
                            "genkai efficiency",
                            comparator(|x| x.efficiency, invert),
                        ).await
                    }
                }?;
            }
        }

        Ok(())
    }

    async fn on_vc_join(&self, ctx: &dyn Context, user_id: u64) -> Result<()> {
        let op = self
            .db
            .create_new_session(user_id, Utc::now())
            .await
            .context("failed to create new session")?;

        if let CreateNewSessionResult::SessionResumed = op {
            let mut timeout = self.resume_msg_timeout.lock().await;
            let now = Utc::now();

            if *timeout < now {
                *timeout = now + *RESUME_MSG_TIMEOUT;
                ctx.send_message(SendMessage {
                    content: &format!("Welcome back <@!{user_id}>, your session has resumed!"),
                    attachments: &[],
                })
                .await
                .context("failed to send message")?;
            }
        }

        Ok(())
    }

    async fn on_vc_leave(&self, ctx: &dyn Context, user_id: u64) -> Result<()> {
        self.db
            .close_session(user_id, Utc::now())
            .await
            .context("failed to close session")?;

        let mut sessions = self
            .db
            .get_users_all_sessions(user_id)
            .await
            .context("failed to get all closed sessions")?;

        sessions.sort_unstable_by_key(|x| x.left_at);

        let last_session = sessions.last().unwrap();
        let this_time_point = last_session.calc_point();

        if this_time_point > 0 {
            let stat = UserStat::from_sessions(&sessions)
                .expect("`sessions` contains multiple user's session")
                .expect("sessions must have 1 elements at least");

            let to_hours = |d: Duration| d.num_minutes() as f64 / 60.0;

            let msg = format!(
                "<@!{uid}>\n限界ポイント: {pt}pt (+{pt_delta}pt)\n総VC時間: {vc_hour:.2}h (+{vc_hour_delta:.2}h)",
                uid = user_id,

                pt = stat.genkai_point,
                pt_delta = this_time_point,

                vc_hour = to_hours(stat.total_vc_duration),
                vc_hour_delta = to_hours(last_session.duration()),
            );

            ctx.send_text_message(&msg)
                .await
                .context("failed to send message")?;
        }

        Ok(())
    }

    async fn on_vc_data_available(
        &self,
        _ctx: &dyn Context,
        joined_user_ids: &[u64],
    ) -> Result<()> {
        for uid in joined_user_ids {
            let op = self
                .db
                .create_new_session(*uid, Utc::now())
                .await
                .context("failed to create new session")?;

            use CreateNewSessionResult::*;

            match op {
                CreatedNewSession | SessionResumed => {
                    tracing::info!("User({}) has joined to vc in bot downtime", uid);
                }

                UnclosedSessionExists => {
                    tracing::info!("User({}) already has unclosed session in db", uid);
                }
            }
        }

        let db_state = self
            .db
            .get_all_users_who_has_unclosed_session()
            .await
            .context("failed to get users who has unclosed session")?;

        for uid in db_state {
            if joined_user_ids.contains(&uid) {
                continue;
            }

            self.db
                .close_session(uid, Utc::now())
                .await
                .context("failed to close session")?;

            tracing::info!("User({}) has left from vc in bot downtime", uid);
        }

        Ok(())
    }
}
