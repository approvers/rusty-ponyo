use {
    crate::{
        bot::{BotService, Context},
        Synced,
    },
    anyhow::{Context as _, Result},
    async_trait::async_trait,
    chrono::{DateTime, Duration, Utc},
    once_cell::sync::Lazy,
    rand::Rng,
    tokio::sync::Mutex,
};

pub(crate) struct VcDiffBot {
    timeout: Mutex<DateTime<Utc>>,
}

static TIMEOUT: Lazy<Duration> = Lazy::new(|| Duration::seconds(3));

impl VcDiffBot {
    pub fn new() -> Self {
        Self {
            timeout: Mutex::new(Utc::now()),
        }
    }

    async fn should_notify(&self) -> bool {
        if !rand::thread_rng().gen_bool(1.0 / 3.0) {
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
    const NAME: &'static str = "VCDiffBot";
    type Database = ();

    async fn on_vc_join(
        &self,
        _db: &Synced<Self::Database>,
        ctx: &dyn Context,
        user_id: u64,
    ) -> Result<()> {
        self.notify(ctx, user_id, true).await
    }

    async fn on_vc_leave(
        &self,
        _db: &Synced<Self::Database>,
        ctx: &dyn Context,
        user_id: u64,
    ) -> Result<()> {
        self.notify(ctx, user_id, false).await
    }
}
