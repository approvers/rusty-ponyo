use {
    crate::{Synced, ThreadSafe},
    anyhow::Result,
};

pub mod alias;
pub(crate) trait Message: ThreadSafe {
    fn content(&self) -> &str;
}

#[async_trait::async_trait]
pub(crate) trait BotService: ThreadSafe {
    type Database: ThreadSafe;

    async fn on_message(
        &self,
        db: &Synced<Self::Database>,
        msg: &dyn Message,
    ) -> Result<Option<String>>;
}
