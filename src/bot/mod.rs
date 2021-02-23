use {
    crate::{Synced, ThreadSafe},
    anyhow::Result,
    async_trait::async_trait,
};

pub mod alias;
pub(crate) trait Message: ThreadSafe {
    fn content(&self) -> &str;
    fn attachments(&self) -> &[&dyn Attachment];
}

#[async_trait]
pub(crate) trait Attachment: ThreadSafe {
    async fn download(&self) -> Result<Vec<u8>>;
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
