use crate::bot::{BotService, Context, Message, Runtime, User, parse_command, ui};
use anyhow::{Context as _, Result};
use rand::seq::SliceRandom;
use rusty_ponyo::KAWAEMON_DISCORD_USER_ID;
use tokio::sync::Mutex;

const NAME: &str = "rusty_ponyo::bot::uo";
const PREFIX: &str = "g!uo";
const UO_DEFAULT: &str = "ｳｰｫ";
const UO_CHOICES: &[&str] = &[
    "ウーォ",
    "ウウーォ",
    "ｳｰｫ",
    "ｩ-ｵ",
    "ｫｰｳ",
    "ｵｰｩ",
    "ｳｫ",
    "ｫ",
    "<:wuo:1321792631465836636>",
    "<:wuo2:1312706662380994571>",
    "<:wuo3:1328363294305550448>",
];

ui! {
    /// ランダムでｳｰｫと言います
    struct Ui {
        name: NAME,
        prefix: PREFIX,
        command: Command,
    }
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    Status,
    Reroll,
}

pub struct UoBot {
    prob_percent: Mutex<u8>,
}

impl UoBot {
    pub fn new() -> Self {
        // TODO: 一日ごとに変更したい
        Self {
            prob_percent: Mutex::new(2),
        }
    }

    async fn reroll(&self) {
        *self.prob_percent.lock().await = 1 + (rand::random::<u8>() % 10);
    }
}

impl<R: Runtime> BotService<R> for UoBot {
    fn name(&self) -> &'static str {
        NAME
    }

    async fn on_message(&self, msg: &R::Message, ctx: &R::Context) -> Result<()> {
        {
            let p = *self.prob_percent.lock().await;
            if rand::random::<f64>() < (p as f64 / 100.0) {
                let uo = UO_CHOICES.choose(&mut rand::thread_rng()).unwrap();
                ctx.send_text_message(uo).await?;
            }
        }

        if !msg.content().starts_with(PREFIX) {
            return Ok(());
        }

        let Some(parsed) = parse_command::<Ui>(msg.content(), ctx).await? else {
            return Ok(());
        };

        let reply = match parsed.command {
            Command::Status => {
                let prob = self.prob_percent.lock().await;
                format!("```{UO_DEFAULT}確率: {prob}%```")
            }
            Command::Reroll => {
                if msg.author().id() == KAWAEMON_DISCORD_USER_ID {
                    self.reroll().await;
                    let prob = self.prob_percent.lock().await;
                    format!("振り直しました: {prob}%")
                } else {
                    "かわえもんでないとこの処理はできません".to_owned()
                }
            }
        };

        msg.reply(&reply).await.context("failed to send message")?;

        Ok(())
    }
}
