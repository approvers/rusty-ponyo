use super::SendAttachment;

mod commands;
pub(crate) mod model;
mod parser;

use {
    crate::{
        bot::{alias::model::MessageAlias, BotService, Context, Message, SendMessage},
        Synced, ThreadSafe,
    },
    anyhow::Result,
    async_trait::async_trait,
    std::marker::PhantomData,
};

const PREFIX: &str = "g!alias";

#[async_trait]
pub(crate) trait MessageAliasDatabase: ThreadSafe {
    async fn save(&mut self, alias: MessageAlias) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Option<MessageAlias>>;
    async fn get_and_increment_usage_count(&mut self, key: &str) -> Result<Option<MessageAlias>>;
    async fn delete(&mut self, key: &str) -> Result<bool>;
    async fn len(&self) -> Result<u32>;
    async fn usage_count_top_n(&self, n: usize) -> Result<Vec<MessageAlias>>;
}

pub(crate) struct MessageAliasBot<D: MessageAliasDatabase>(PhantomData<fn() -> D>);

#[async_trait]
impl<D: MessageAliasDatabase> BotService for MessageAliasBot<D> {
    const NAME: &'static str = "MessageAliasBot";
    type Database = D;

    async fn on_message(
        &self,
        db: &Synced<Self::Database>,
        msg: &dyn Message,
        ctx: &dyn Context,
    ) -> Result<()> {
        // TODO: support commandAdd?
        if msg.content().starts_with(PREFIX) {
            if let Some(msg) = self.on_command(db, msg).await? {
                ctx.send_message(SendMessage {
                    content: &msg,
                    attachments: &[],
                })
                .await?;
            }
        }

        if let Some(registered_alias) = db
            .write()
            .await
            .get_and_increment_usage_count(msg.content())
            .await?
        {
            ctx.send_message(SendMessage {
                content: &registered_alias.message,
                attachments: &registered_alias
                    .attachments
                    .iter()
                    .map(|x| SendAttachment {
                        name: &x.name,
                        data: &x.data,
                    })
                    .collect::<Vec<_>>(),
            })
            .await?;
        }

        Ok(())
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
            "help" => Ok(Some(help())),

            "delete" => {
                let key = parsed.args.get(0);

                if let Some(key) = key {
                    return delete(db, key).await.map(Some);
                }

                Ok(Some(help()))
            }

            "make" => {
                let key = parsed.args.get(0);
                let value = parsed.args.get(1).map(|x| x.as_str());

                if let Some(key) = key {
                    return make(db, key, value, message.attachments()).await.map(Some);
                }

                Ok(Some(help()))
            }

            "status" => Ok(Some(status(db).await?)),
            "ranking" => Ok(Some(usage_ranking(db).await?)),

            _ => Ok(Some(help())),
        }
    }
}
