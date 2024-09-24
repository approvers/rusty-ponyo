mod bot;
mod client;
mod db;

use {
    crate::bot::{
        alias::MessageAliasBot,
        auth::GenkaiAuthBot,
        genkai_point::{plot, GenkaiPointBot},
        gh::GitHubCodePreviewBot,
        vc_diff::VcDiffBot,
    },
    anyhow::{Context as _, Result},
    bot::meigen::MeigenBot,
};

assert_one_feature!("discord_client", "console_client");
assert_one_feature!("mongo_db", "memory_db");
assert_one_feature!("plot_plotters", "plot_matplotlib", "plot_charming");

fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let use_ansi = env_var("NO_COLOR").is_err();
    tracing_subscriber::fmt().with_ansi(use_ansi).init();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main())
}

async fn async_main() -> Result<()> {
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

    #[cfg(feature = "plot_plotters")]
    let plotter = plot::plotters::Plotters::new();
    #[cfg(feature = "plot_matplotlib")]
    let plotter = plot::matplotlib::Matplotlib::new();
    #[cfg(feature = "plot_charming")]
    let plotter = plot::charming::Charming::new();

    let pgp_whitelist = env_var("PGP_SOURCE_DOMAIN_WHITELIST")?
        .split(',')
        .map(|x| x.to_string())
        .collect();

    client
        .add_service(MessageAliasBot::new(local_db.clone()))
        .add_service(GenkaiPointBot::new(local_db.clone(), plotter))
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

macro_rules! assert_one_feature {
    ($a:literal, $b: literal) => {
        #[cfg(all(feature = $a, feature = $b))]
        compile_error!(concat!(
            "You can't enable both of ",
            $a,
            " and ",
            $b,
            " feature at the same time."
        ));

        #[cfg(not(any(feature = $a, feature = $b)))]
        compile_error!(concat!(
            "You must enable either ",
            $a,
            " or ",
            $b,
            " feature."
        ));
    };
    ($a:literal, $b:literal, $c:literal) => {
        #[cfg(all(feature = $a, feature = $b, feature = $c))]
        compile_error!(concat!(
            "You can't enable both of ",
            $a,
            " and ",
            $b,
            " and ",
            $c,
            " feature at the same time."
        ));

        #[cfg(all(feature = $a, feature = $b))]
        compile_error!(concat!(
            "You can't enable both of ",
            $a,
            " and ",
            $b,
            " feature at the same time."
        ));

        #[cfg(all(feature = $b, feature = $c))]
        compile_error!(concat!(
            "You can't enable both of ",
            $b,
            " and ",
            $c,
            " feature at the same time."
        ));

        #[cfg(all(feature = $c, feature = $a))]
        compile_error!(concat!(
            "You can't enable both of ",
            $c,
            " and ",
            $a,
            " feature at the same time."
        ));

        #[cfg(not(any(feature = $a, feature = $b, feature = $c)))]
        compile_error!(concat!(
            "You must enable either ",
            $a,
            " or ",
            $b,
            " or ",
            $c,
            " feature."
        ));
    };
}

use assert_one_feature;
