#![feature(let_else)]

mod bot;
mod client;
mod db;

use {
    crate::bot::{alias::MessageAliasBot, auth::GenkaiAuthBot, genkai_point::GenkaiPointBot, gh::GitHubCodePreviewBot},
    anyhow::{Context as _, Result},
};

#[rustfmt::skip]
#[cfg(all(feature = "discord_client", feature = "console_client"))]
compile_error!("You can't enable both of discord_client and console_client feature at the same time.");

#[cfg(all(feature = "mongo_db", feature = "memory_db"))]
compile_error!("You can't enable both of mongo_db and memory_db feature at the same time.");

#[cfg(not(any(feature = "discord_client", feature = "console_client")))]
compile_error!("You must enable discord_client or console_client feature.");

#[cfg(not(any(feature = "mongo_db", feature = "memory_db")))]
compile_error!("You must enable mongo_db or memory_db feature.");

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let use_ansi = env_var("NO_COLOR").is_err();

    tracing_subscriber::fmt().with_ansi(use_ansi).init();

    #[cfg(feature = "memory_db")]
    let db = crate::db::mem::MemoryDB::new();
    #[cfg(feature = "memory_db")]
    let auth_db = db.clone();

    #[cfg(feature = "mongo_db")]
    let db = crate::db::mongodb::MongoDb::new(&env_var("MONGODB_URI")?).await?;
    #[cfg(feature = "mongo_db")]
    let auth_db = crate::db::mongodb::MongoDb::new(&env_var("MONGO_AUTH_DB_URI")?).await?;

    #[cfg(feature = "console_client")]
    let mut client = crate::client::console::ConsoleClient::new();
    #[cfg(feature = "discord_client")]
    let mut client = crate::client::discord::DiscordClient::new();

    let pgp_whitelist = env_var("PGP_SOURCE_DOMAIN_WHITELIST")?
        .split(',')
        .map(|x| x.to_string())
        .collect();

    client
        .add_service(MessageAliasBot::new(db.clone()))
        .add_service(GenkaiPointBot::new(db.clone()))
        .add_service(GitHubCodePreviewBot)
        .add_service(GenkaiAuthBot::new(auth_db, pgp_whitelist));

    #[cfg(feature = "console_client")]
    client.run().await?;
    #[cfg(feature = "discord_client")]
    client.run(&env_var("DISCORD_TOKEN")?).await?;

    Ok(())
}

fn env_var(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("failed to get {} environment variable", name))
}
