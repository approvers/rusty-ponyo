use {
    crate::bot::{parse_command, ui, BotService, Context, Message},
    anyhow::{Context as _, Result},
    async_trait::async_trait,
    chrono::{DateTime, Duration, Utc},
    once_cell::sync::Lazy,
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
    /// ヘルプメッセージを出します
    Help,

    /// 機能を有効化します
    Enable,

    /// 機能を無効化します
    Disable,
}

pub(crate) struct VcDiffBot {
    enabled: Mutex<bool>,
    timeout: Mutex<DateTime<Utc>>,
}

static TIMEOUT: Lazy<Duration> = Lazy::new(|| Duration::seconds(1));

impl VcDiffBot {
    pub fn new() -> Self {
        Self {
            enabled: Mutex::new(false),
            timeout: Mutex::new(Utc::now()),
        }
    }

    async fn should_notify(&self) -> bool {
        if !*self.enabled.lock().await {
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

    async fn notify(&self, ctx: &dyn Context, user_id: u64, joined: bool) -> Result<()> {
        if !self.should_notify().await {
            return Ok(());
        }

        let name = ctx
            .get_user_name(user_id)
            .await
            .context("failed to get user name")?;

        let msg = if joined {
            format!("{}がVCに入りました", name)
        } else {
            format!("{}がVCから抜けました", name)
        };

        ctx.send_text_message(&msg)
            .await
            .context("failed to send message")?;

        Ok(())
    }
}

#[async_trait]
impl BotService for VcDiffBot {
    fn name(&self) -> &'static str {
        NAME
    }

    async fn on_message(&self, msg: &dyn Message, ctx: &dyn Context) -> Result<()> {
        if !msg.content().starts_with(PREFIX) {
            return Ok(());
        }

        let Some(parsed) = parse_command::<Ui>(msg.content(), ctx).await?
            else { return Ok(()) };

        use Command::*;

        let msg = match parsed.command {
            // handled by clap
            Help => return Ok(()),

            Enable => {
                *self.enabled.lock().await = true;
                "vcdiff を有効化しました"
            }

            Disable => {
                *self.enabled.lock().await = false;
                "vcdiff を無効化しました"
            }
        };

        ctx.send_text_message(msg)
            .await
            .context("failed to send message")?;

        Ok(())
    }

    async fn on_vc_join(&self, ctx: &dyn Context, user_id: u64) -> Result<()> {
        self.notify(ctx, user_id, true).await
    }

    async fn on_vc_leave(&self, ctx: &dyn Context, user_id: u64) -> Result<()> {
        self.notify(ctx, user_id, false).await
    }
}
