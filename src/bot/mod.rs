use {anyhow::Result, async_trait::async_trait, std::future::Future, std::pin::Pin};

pub mod alias;
pub mod auth;
pub mod genkai_point;

pub(crate) trait Message: Send + Sync {
    fn author(&self) -> &dyn User;
    fn content(&self) -> &str;
    fn attachments(&self) -> &[&dyn Attachment];
}

#[async_trait]
pub(crate) trait Attachment: Send + Sync {
    fn name(&self) -> &str;
    fn size(&self) -> usize;
    async fn download(&self) -> Result<Vec<u8>>;
}

#[async_trait]
pub(crate) trait User: Send + Sync {
    fn id(&self) -> u64;
    fn name(&self) -> &str;
    async fn dm(&self, msg: SendMessage<'_>) -> Result<()>;

    fn dm_text<'a>(
        &'a self,
        text: &'a str,
    ) -> Pin<Box<dyn Send + Future<Output = Result<()>> + 'a>> {
        self.dm(SendMessage {
            content: text,
            attachments: &[],
        })
    }
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
pub(crate) trait Context: Send + Sync {
    async fn send_message(&self, msg: SendMessage<'_>) -> Result<()>;
    async fn get_user_name(&self, user_id: u64) -> Result<String>;

    fn send_text_message<'a>(
        &'a self,
        text: &'a str,
    ) -> Pin<Box<dyn Send + Future<Output = Result<()>> + 'a>> {
        self.send_message(SendMessage {
            content: text,
            attachments: &[],
        })
    }
}

#[async_trait]
pub(crate) trait BotService: Send + Sync {
    fn name(&self) -> &'static str;

    async fn on_message(&self, _msg: &dyn Message, _ctx: &dyn Context) -> Result<()> {
        Ok(())
    }

    // called on bot started and got who is currently joined to vc
    async fn on_vc_data_available(
        &self,
        _ctx: &dyn Context,
        _joined_user_ids: &[u64],
    ) -> Result<()> {
        Ok(())
    }

    // called on user has joined to vc
    async fn on_vc_join(&self, _ctx: &dyn Context, _user_id: u64) -> Result<()> {
        Ok(())
    }

    // called on user has left from vc and not in any channel
    async fn on_vc_leave(&self, _ctx: &dyn Context, _user_id: u64) -> Result<()> {
        Ok(())
    }
}
