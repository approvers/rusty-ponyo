pub mod console;
pub mod discord;

use {
    crate::{bot::BotService, Synced, ThreadSafe},
    anyhow::Result,
    async_trait::async_trait,
};

struct ServiceEntryInner<S, D> {
    service: S,
    db: Synced<D>,
}

#[async_trait]
trait ServiceEntry: ThreadSafe {
    async fn on_message(&self, msg: &str) -> Result<Option<String>>;
}

#[async_trait]
impl<S, D> ServiceEntry for ServiceEntryInner<S, D>
where
    S: BotService<Database = D>,
    D: ThreadSafe,
{
    async fn on_message(&self, msg: &str) -> Result<Option<String>> {
        self.service.on_message(&self.db, msg).await
    }
}
