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
}
