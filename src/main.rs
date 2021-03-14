#![allow(dead_code)]

mod bot;
mod client;
mod db;

use {
    crate::bot::{alias::MessageAliasBot, genkai_point::GenkaiPointBot},
    anyhow::{Context as _, Result},
    cfg_if::cfg_if,
    std::sync::Arc,
    tokio::sync::RwLock,
};

#[rustfmt::skip]
#[cfg(all(feature = "discord_client", feature = "console_client"))]
compile_error!("You can't enable both of discord_client and console_client feature at the same time.");

#[cfg(all(feature = "mongo_db", feature = "memory_db"))]
compile_error!("You can't enable both of mongo_db and memory_db feature at the same time.");

#[cfg(not(any(feature = "discord_client", feature = "console_client")))]
compile_error!("You must enable one of discord_client or console_client feature.");

#[cfg(not(any(feature = "mongo_db", feature = "memory_db")))]
compile_error!("You must enable mongo_db or memory_db feature.");

type Synced<T> = Arc<RwLock<T>>;

trait ThreadSafe: Send + Sync {}
impl<T> ThreadSafe for T where T: Send + Sync {}

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
    let db = {
        cfg_if! {
            if #[cfg(feature = "memory_db")] {
                crate::db::mem::MemoryDB::new()
            } else if #[cfg(feature = "mongo_db")] {
                crate::db::mongodb::MongoDb::new(&env_var("MONGODB_URI")?).await?
            } else {
                compile_error!();
            }
        }
    };

    let db = Arc::new(RwLock::new(db));

    let mut client = {
        cfg_if! {
            if #[cfg(feature = "console_client")] {
                crate::client::console::ConsoleClient::new()
            } else if #[cfg(feature = "discord_client")] {
                crate::client::discord::DiscordClient::new()
            } else {
                compile_error!()
            }
        }
    };

    client
        .add_service(MessageAliasBot::new(), Arc::clone(&db))
        .add_service(GenkaiPointBot::new(), Arc::clone(&db));

    cfg_if! {
        if #[cfg(feature = "console_client")] {
            client.run().await
        } else if #[cfg(feature = "discord_client")] {
            client.run(&env_var("DISCORD_TOKEN")?).await
        } else {
            compile_error!()
        }
    }
}
