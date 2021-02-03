use {
    crate::{Synced, ThreadSafe},
    anyhow::Result,
};

pub mod alias;

#[async_trait::async_trait]
pub(crate) trait BotService: ThreadSafe {
    type Database: ThreadSafe;

    async fn on_message(&self, db: &Synced<Self::Database>, msg: &str) -> Result<Option<String>>;
}
