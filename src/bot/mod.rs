use {
    crate::{Synced, ThreadSafe},
    anyhow::Result,
    async_trait::async_trait,
};

pub mod alias;
pub mod auth;
pub mod genkai_point;

pub(crate) trait Message: ThreadSafe {
    fn author(&self) -> &dyn User;
    fn content(&self) -> &str;
    fn attachments(&self) -> &[&dyn Attachment];
}

#[async_trait]
pub(crate) trait Attachment: ThreadSafe {
    fn name(&self) -> &str;
    async fn download(&self) -> Result<Vec<u8>>;
}

#[async_trait]
pub(crate) trait User: ThreadSafe {
    fn id(&self) -> u64;
    fn name(&self) -> &str;
    async fn dm(&self, msg: SendMessage<'_>) -> Result<()>;
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
    async fn get_user_name(&self, user_id: u64) -> Result<String>;
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

    // called on bot started and got who is currently joined to vc
    async fn on_vc_data_available(
        &self,
        _db: &Synced<Self::Database>,
        _ctx: &dyn Context,
        _joined_user_ids: &[u64],
    ) -> Result<()> {
        Ok(())
    }

    // called on user has joined to vc
    async fn on_vc_join(
        &self,
        _db: &Synced<Self::Database>,
        _ctx: &dyn Context,
        _user_id: u64,
    ) -> Result<()> {
        Ok(())
    }

    // called on user has left from vc and not in any channel
    async fn on_vc_leave(
        &self,
        _db: &Synced<Self::Database>,
        _ctx: &dyn Context,
        _user_id: u64,
    ) -> Result<()> {
        Ok(())
    }
}
