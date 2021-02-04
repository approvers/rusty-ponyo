#![allow(dead_code)]

mod bot;
mod client;
mod db;
mod model;

use {
    anyhow::{Context as _, Result},
    std::sync::Arc,
    tokio::sync::RwLock,
};

type Synced<T> = Arc<RwLock<T>>;

trait ThreadSafe: Send + Sync + 'static {}
impl<T> ThreadSafe for T where T: Send + Sync + 'static {}

fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let use_ansi = env_var("NO_COLOR").is_err();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_ansi(use_ansi)
        .init();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to build tokio runtime")?
        .block_on(async_main())
}

fn env_var(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("failed to get {} environment variable", name))
}

async fn async_main() -> Result<()> {
    let token = env_var("DISCORD_TOKEN")?;
    crate::client::discord::DiscordClient::new(&token)
        .add_service(
            crate::bot::alias::MessageAliasBot::new(),
            Arc::new(RwLock::new(crate::db::mem::MemoryDB::new())),
        )
        .run()
        .await?;

    Ok(())
}
