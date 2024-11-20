use crate::bot::{parse_command, ui, BotService, Context, Message};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use tokio::sync::Mutex;

const NAME: &str = "rusty_ponyo::bot::uo";
const PREFIX: &str = "g!uo";
const UO: &str = "ｳｰｫ";

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

pub(crate) struct UoBot {
    prob_percent: Mutex<u8>,
}

impl UoBot {
    pub fn new() -> Self {
        Self {
            prob_percent: Mutex::new(3),
        }
    }

    async fn reroll(&self) {
        *self.prob_percent.lock().await = 1 + (rand::random::<u8>() % 10);
    }
}

#[async_trait]
impl BotService for UoBot {
    fn name(&self) -> &'static str {
        NAME
    }

    async fn on_message(&self, msg: &dyn Message, ctx: &dyn Context) -> Result<()> {
        {
            let p = *self.prob_percent.lock().await;
            if rand::random::<f64>() < (p as f64 / 100.0) {
                msg.reply(UO).await?;
            }
        }

        if !msg.content().starts_with(PREFIX) {
            return Ok(());
        }

        let Some(parsed) = parse_command::<Ui>(msg.content(), ctx).await? else {
            return Ok(());
        };

        use Command::*;

        let msg = match parsed.command {
            Status => {
                let prob = self.prob_percent.lock().await;
                format!("```{UO}確率: {prob}%```")
            }
            Reroll => {
                const KAWAEMON_DISCORD_USER_ID: u64 = 391857452360007680;
                if msg.author().id() == KAWAEMON_DISCORD_USER_ID {
                    self.reroll().await;
                    "振り直しました".to_owned()
                } else {
                    "かわえもんでないとこの処理はできません".to_owned()
                }
            }
        };

        ctx.send_text_message(&msg)
            .await
            .context("failed to send message")?;

        Ok(())
    }
}
