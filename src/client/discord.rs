use {
    crate::{
        bot::{Attachment, BotService, Context, Message, SendMessage},
        client::{ServiceEntry, ServiceEntryInner},
        Synced, ThreadSafe,
    },
    anyhow::{Context as _, Result},
    async_trait::async_trait,
    serenity::{
        http::AttachmentType,
        model::{
            channel::{Attachment as SerenityAttachment, Message as SerenityMessage},
            gateway::Ready,
            id::ChannelId as SerenityChannelId,
        },
        prelude::{Client, Context as SerenityContext, EventHandler},
    },
};

pub(crate) struct DiscordClient {
    services: Vec<Box<dyn ServiceEntry>>,
}

impl DiscordClient {
    pub fn new() -> Self {
        Self { services: vec![] }
    }

    pub fn add_service<S, D>(mut self, service: S, db: Synced<D>) -> Self
    where
        S: BotService<Database = D> + 'static,
        D: ThreadSafe + 'static,
    {
        self.services
            .push(Box::new(ServiceEntryInner { service, db }));
        self
    }

    pub async fn run(self, token: &str) -> Result<()> {
        let event_handler = EvHandler {
            services: self.services,
        };

        Client::builder(token)
            .event_handler(event_handler)
            .await
            .context("Failed to create Discord client")?
            .start()
            .await
            .context("Failed to start Discord client")
    }
}

struct EvHandler {
    services: Vec<Box<dyn ServiceEntry>>,
}

#[async_trait]
impl EventHandler for EvHandler {
    async fn ready(&self, _: SerenityContext, ready: Ready) {
        tracing::info!("DiscordBot({}) is connected!", ready.user.name);
    }

    async fn message(&self, ctx: SerenityContext, message: SerenityMessage) {
        let converted_attachments = message
            .attachments
            .into_iter()
            .map(DiscordAttachment)
            .collect::<Vec<_>>();

        let converted_message = DiscordMessage {
            content: message.content.clone(),
            attachments: converted_attachments.iter().map(|x| x as _).collect(),
        };

        let converted_context = DiscordContext {
            origin: ctx,
            channel_id: message.channel_id,
        };

        for service in &self.services {
            let result = service
                .on_message(&converted_message, &converted_context)
                .await;

            if let Err(err) = result {
                tracing::error!(
                    "Error occur while running command '{}'\n{:?}",
                    &message.content,
                    err
                );
            }
        }
    }
}

struct DiscordMessage<'a> {
    content: String,
    attachments: Vec<&'a dyn Attachment>,
}

impl Message for DiscordMessage<'_> {
    fn content(&self) -> &str {
        &self.content
    }

    fn attachments(&self) -> &[&dyn Attachment] {
        &self.attachments
    }
}

struct DiscordAttachment(SerenityAttachment);

#[async_trait]
impl Attachment for DiscordAttachment {
    fn name(&self) -> &str {
        &self.0.filename
    }

    async fn download(&self) -> Result<Vec<u8>> {
        self.0
            .download()
            .await
            .context("failed to download attachment from discord")
    }
}

struct DiscordContext {
    origin: SerenityContext,
    channel_id: SerenityChannelId,
}

#[async_trait]
impl Context for DiscordContext {
    async fn send_message(&self, msg: SendMessage<'_>) -> Result<()> {
        #[rustfmt::skip]
        let files = msg
            .attachments
            .iter()
            .map(|x| AttachmentType::Bytes {
                data: x.data.into(),
                filename: x.name.to_string(),
            })
            .collect::<Vec<_>>();

        self.channel_id
            .send_files(&self.origin.http, files, |m| m.content(msg.content))
            .await
            .context("failed to send message to discord")?;

        Ok(())
    }
}
