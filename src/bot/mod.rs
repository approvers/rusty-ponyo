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
    fn name(&self) -> &str;
    async fn download(&self) -> Result<Vec<u8>>;
}

pub(crate) struct SendMessage<'a> {
    pub(crate) content: &'a str,
    pub(crate) attachments: &'a [SendAttachment<'a>],
}

pub(crate) struct SendAttachment<'a> {
    pub(crate) name: &'a str,
    pub(crate) data: &'a [u8],
}

#[async_trait]
pub(crate) trait Context: ThreadSafe {
    async fn send_message(&self, msg: SendMessage<'_>) -> Result<()>;
}

#[async_trait]
pub(crate) trait BotService: ThreadSafe {
    const NAME: &'static str;
    type Database: ThreadSafe;

    async fn on_message(
        &self,
        db: &Synced<Self::Database>,
        msg: &dyn Message,
        ctx: &dyn Context,
    ) -> Result<()>;
}
