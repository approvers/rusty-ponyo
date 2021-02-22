use {
    crate::{
        bot::BotService,
        client::{Message, ServiceEntry, ServiceEntryInner},
        Synced, ThreadSafe,
    },
    anyhow::{Context as _, Result},
    async_trait::async_trait,
    serenity::{
        model::{channel::Message as SerenityMessage, gateway::Ready},
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
        S: BotService<Database = D>,
        D: ThreadSafe,
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
        let message = DiscordMessage { origin: message };

        for service in &self.services {
            let result = service.on_message(&message).await;

            match result {
                Err(err) => tracing::error!(
                    "Error occur while running command '{}'\n{:?}",
                    &message.origin.content,
                    err
                ),

                Ok(Some(text)) => {
                    if let Err(e) = message.origin.channel_id.say(&ctx.http, &text).await {
                        tracing::error!("Error occur while sending message {:?},'{}'", e, text);
                    }
                }
                _ => {}
            }
        }
    }
}

struct DiscordMessage {
    origin: SerenityMessage,
}

impl Message for DiscordMessage {
    fn content(&self) -> &str {
        &self.origin.content
    }
}
