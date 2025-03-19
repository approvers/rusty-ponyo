use {
    crate::bot::{BotService, Context, Message, Runtime, parse_command, ui},
    anyhow::{Context as _, Result},
    chrono::{DateTime, Duration, Utc},
    once_cell::sync::Lazy,
    std::sync::atomic::AtomicBool,
    std::sync::atomic::Ordering::Relaxed,
    tokio::sync::Mutex,
};

const NAME: &str = "rusty_ponyo::bot::vc_diff";
const PREFIX: &str = "g!vcdiff";

ui! {
    /// VC の入退出を通知します。はらちょがいないとき用です。
    struct Ui {
        name: NAME,
        prefix: PREFIX,
        command: Command,
    }
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    /// 機能を有効化します
    Enable,

    /// 機能を無効化します
    Disable,

    /// 現在機能が有効か無効かを表示します
    Status,
}

pub struct VcDiffBot {
    enabled: AtomicBool,
    timeout: Mutex<DateTime<Utc>>,
}

static TIMEOUT: Lazy<Duration> = Lazy::new(|| Duration::seconds(1));

impl VcDiffBot {
    pub fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            timeout: Mutex::new(Utc::now()),
        }
    }

    async fn should_notify(&self) -> bool {
        if !self.enabled.load(Relaxed) {
            return false;
        }

        let mut timeout = self.timeout.lock().await;

        let now = Utc::now();
        if *timeout >= now {
            return false;
        }

        *timeout = now + *TIMEOUT;
        true
    }

    async fn notify(&self, ctx: &impl Context, user_id: u64, joined: bool) -> Result<()> {
        if !self.should_notify().await {
            return Ok(());
        }

        let name = ctx
            .get_user_name(user_id)
            .await
            .context("failed to get user name")?;

        let msg = if joined {
            format!("{name}がVCに入りました")
        } else {
            format!("{name}がVCから抜けました")
        };

        ctx.send_text_message(&msg)
            .await
            .context("failed to send message")?;

        Ok(())
    }
}

impl<R: Runtime> BotService<R> for VcDiffBot {
    fn name(&self) -> &'static str {
        NAME
    }

    async fn on_message(&self, msg: &R::Message, ctx: &R::Context) -> Result<()> {
        if !msg.content().starts_with(PREFIX) {
            return Ok(());
        }

        let Some(parsed) = parse_command::<Ui>(msg.content(), ctx).await? else {
            return Ok(());
        };

        use Command::*;

        let msg = match parsed.command {
            Enable => {
                self.enabled.store(true, Relaxed);
                "vcdiff を有効化しました"
            }

            Disable => {
                self.enabled.store(false, Relaxed);
                "vcdiff を無効化しました"
            }

            Status => {
                if self.enabled.load(Relaxed) {
                    "vcdiff は現在有効です"
                } else {
                    "vcdiff は現在無効です"
                }
            }
        };

        ctx.send_text_message(msg)
            .await
            .context("failed to send message")?;

        Ok(())
    }

    async fn on_vc_join(&self, ctx: &R::Context, user_id: u64) -> Result<()> {
        self.notify(ctx, user_id, true).await
    }

    async fn on_vc_leave(&self, ctx: &R::Context, user_id: u64) -> Result<()> {
        self.notify(ctx, user_id, false).await
    }
}
