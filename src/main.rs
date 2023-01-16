#![feature(let_chains)]
#![feature(if_let_guard)]
#![deny(warnings)]

mod bot;
mod client;
mod db;

use {
    crate::bot::{
        alias::MessageAliasBot, auth::GenkaiAuthBot, genkai_point::GenkaiPointBot,
        gh::GitHubCodePreviewBot, vc_diff::VcDiffBot,
    },
    anyhow::{Context as _, Result},
    bot::meigen::MeigenBot,
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
    let local_db = crate::db::mem::MemoryDB::new();
    #[cfg(feature = "memory_db")]
    let remote_db = local_db.clone();

    #[cfg(feature = "mongo_db")]
    let local_db = crate::db::mongodb::MongoDb::new(&env_var("MONGODB_URI")?).await?;
    #[cfg(feature = "mongo_db")]
    let remote_db = crate::db::mongodb::MongoDb::new(&env_var("MONGODB_ATLAS_URI")?).await?;

    #[cfg(feature = "console_client")]
    let mut client = crate::client::console::ConsoleClient::new();
    #[cfg(feature = "discord_client")]
    let mut client = crate::client::discord::DiscordClient::new();

    let pgp_whitelist = env_var("PGP_SOURCE_DOMAIN_WHITELIST")?
        .split(',')
        .map(|x| x.to_string())
        .collect();

    client
        .add_service(MessageAliasBot::new(local_db.clone()))
        .add_service(GenkaiPointBot::new(local_db.clone()))
        .add_service(GitHubCodePreviewBot)
        .add_service(GenkaiAuthBot::new(remote_db.clone(), pgp_whitelist))
        .add_service(MeigenBot::new(remote_db))
        .add_service(VcDiffBot::new());

    #[cfg(feature = "console_client")]
    client.run().await?;
    #[cfg(feature = "discord_client")]
    client.run(&env_var("DISCORD_TOKEN")?).await?;

    Ok(())
}

fn env_var(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("failed to get {name} environment variable"))
}
