#[cfg(feature = "console_client")]
pub mod console;

#[cfg(feature = "discord_client")]
pub mod discord;

use {
    crate::{
        bot::{BotService, Context, Message},
        Synced, ThreadSafe,
    },
    anyhow::Result,
    async_trait::async_trait,
};

struct ServiceEntryInner<S, D> {
    service: S,
    db: Synced<D>,
}

#[async_trait]
trait ServiceEntry: ThreadSafe {
    fn name(&self) -> &'static str;

    async fn on_message(&self, msg: &dyn Message, ctx: &dyn Context) -> Result<()>;
    async fn on_vc_data_available(&self, ctx: &dyn Context, joined_user_ids: &[u64]) -> Result<()>;
    async fn on_vc_join(&self, ctx: &dyn Context, user_id: u64) -> Result<()>;
    async fn on_vc_leave(&self, ctx: &dyn Context, user_id: u64) -> Result<()>;
}

#[async_trait]
impl<S, D> ServiceEntry for ServiceEntryInner<S, D>
where
    S: BotService<Database = D>,
    D: ThreadSafe,
{
    #[inline]
    fn name(&self) -> &'static str {
        S::NAME
    }

    async fn on_message(&self, msg: &dyn Message, ctx: &dyn Context) -> Result<()> {
        self.service.on_message(&self.db, msg, ctx).await
    }

    async fn on_vc_data_available(&self, ctx: &dyn Context, joined_user_ids: &[u64]) -> Result<()> {
        self.service
            .on_vc_data_available(&self.db, ctx, joined_user_ids)
            .await
    }

    async fn on_vc_join(&self, ctx: &dyn Context, user_id: u64) -> Result<()> {
        self.service.on_vc_join(&self.db, ctx, user_id).await
    }

    async fn on_vc_leave(&self, ctx: &dyn Context, user_id: u64) -> Result<()> {
        self.service.on_vc_leave(&self.db, ctx, user_id).await
    }
}
