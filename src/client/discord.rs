use {
    crate::{
        bot::{Attachment, BotService, Message},
        client::{ServiceEntry, ServiceEntryInner},
        Synced, ThreadSafe,
    },
    anyhow::{Context as _, Result},
    async_trait::async_trait,
    parking_lot::Mutex,
    serenity::{
        model::{
            channel::{Attachment as SerenityAttachment, Message as SerenityMessage},
            gateway::Ready,
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

        for service in &self.services {
            let result = service.on_message(&converted_message).await;

            match result {
                Err(err) => tracing::error!(
                    "Error occur while running command '{}'\n{:?}",
                    &message.content,
                    err
                ),

                Ok(Some(text)) => {
                    if let Err(e) = message.channel_id.say(&ctx.http, &text).await {
                        tracing::error!("Error occur while sending message {:?},'{}'", e, text);
                    }
                }
                _ => {}
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
