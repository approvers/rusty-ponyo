use {
    anyhow::{Context as _, Result},
    async_trait::async_trait,
    serenity::{
        client::{Client, Context, EventHandler},
        model::{channel::Message, gateway::Ready},
    },
    std::sync::Arc,
    tokio::sync::RwLock,
};

type Synced<T> = Arc<RwLock<T>>;

fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let use_ansi = env_var("NO_COLOR").is_err();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_ansi(use_ansi)
        .init();

    tokio::runtime::Builder::new()
        .threaded_scheduler()
        .enable_all()
        .build()
        .context("failed to build tokio runtime")?
        .block_on(async_main())
}

fn env_var(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("failed to get {} environment variable", name))
}

async fn async_main() -> Result<()> {
    let discord_token = env_var("DISCORD_TOKEN")?;

    Client::builder(&discord_token)
        .event_handler(DiscordEventHandler)
        .await
        .context("Failed to create Discord Client")?
        .start()
        .await
        .context("Client Error")?;

    Ok(())
}

struct DiscordEventHandler;

#[async_trait]
impl EventHandler for DiscordEventHandler {
    async fn ready(&self, _: Context, ready: Ready) {
        log::info!("DiscordBot({}) is connected!", ready.user.name);
    }

    async fn message(&self, ctx: Context, msg: Message) {
        todo!("message handler");
    }
}
