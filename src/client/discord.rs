use {
    crate::{
        bot::BotService,
        client::{ServiceEntry, ServiceEntryInner},
        Synced, ThreadSafe,
    },
    anyhow::{Context as _, Result},
    async_trait::async_trait,
    serenity::{
        model::{channel::Message, gateway::Ready},
        prelude::{Client, Context as SerenityContext, EventHandler},
    },
};

pub(crate) struct DiscordClient<'token> {
    services: Vec<Box<dyn ServiceEntry>>,
    token: &'token str,
}

impl<'token> DiscordClient<'token> {
    pub fn new(token: &'token str) -> Self {
        Self {
            services: vec![],
            token,
        }
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

    pub async fn run(self) -> Result<()> {
        let event_handler = EvHandler {
            services: self.services,
        };

        Client::builder(self.token)
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

    async fn message(&self, ctx: SerenityContext, message: Message) {
        for service in &self.services {
            let result = service.on_message(&message.content).await;

            match result {
                Err(err) => tracing::error!(
                    "Error occur while running command '{}'\n{:?}",
                    message.content,
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
