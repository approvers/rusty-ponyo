mod commands;
mod parser;

use {
    crate::{
        bot::{BotService, Message},
        db::MessageAliasDatabase,
        Synced,
    },
    anyhow::Result,
    std::marker::PhantomData,
};

const PREFIX: &str = "g!alias";

pub(crate) struct MessageAliasBot<D: MessageAliasDatabase>(PhantomData<fn() -> D>);

#[async_trait::async_trait]
impl<D: MessageAliasDatabase> BotService for MessageAliasBot<D> {
    type Database = D;

    async fn on_message(
        &self,
        db: &Synced<Self::Database>,
        msg: &dyn Message,
    ) -> Result<Option<String>> {
        // TODO: support commandAdd?
        if msg.content().starts_with(PREFIX) {
            return self.on_command(db, msg).await;
        }

        let fetched = db.read().await.get(msg.content()).await?;

        if let Some(msg) = fetched {
            return Ok(Some(msg));
        }

        Ok(None)
    }
}

impl<D: MessageAliasDatabase> MessageAliasBot<D> {
    pub(crate) fn new() -> Self {
        Self(PhantomData)
    }

    async fn on_command(&self, db: &Synced<D>, message: &dyn Message) -> Result<Option<String>> {
        use commands::*;

        let parsed = match parser::parse(message.content()) {
            Ok(Some(p)) => p,
            // syntax error
            Err(e) => return Ok(Some(e)),
            _ => return Ok(None),
        };

        if parsed.sub_command.is_none() {
            return Ok(Some(help()));
        }

        match parsed.sub_command.unwrap() {
            "help" => return Ok(Some(help())),

            "delete" => {
                let key = parsed.args.get(0);

                if let Some(key) = key {
                    return delete(db, key).await.map(Some);
                }

                return Ok(Some(help()));
            }

            "make" => {
                let key = parsed.args.get(0);
                let value = parsed.args.get(1);

                if let (Some(key), Some(value)) = (key, value) {
                    return make(db, key, value).await.map(Some);
                }

                return Ok(Some(help()));
            }

            _ => return Ok(Some(help())),
        }
    }
}
