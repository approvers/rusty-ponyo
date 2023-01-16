mod command;
pub(crate) mod model;

use {
    crate::bot::{
        alias::model::MessageAlias, parse_command, ui, BotService, Context, IsUpdated, Message,
        SendAttachment, SendMessage,
    },
    anyhow::Result,
    async_trait::async_trait,
};

const NAME: &str = "rusty_ponyo::bot::alias";
const PREFIX: &str = "g!alias";

ui! {
    /// 特定のメッセージが送信されたときに、指定されたメッセージを同じ場所に送信します。
    struct Ui {
        name: NAME,
        prefix: PREFIX,
        command: Command,
    }
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    /// 表示回数が多い順のランキングを出します
    Ranking,

    /// 現在登録されているエイリアス数を出します
    Status,

    /// エイリアスを削除します
    Delete {
        /// 消したいエイリアスのキー
        key: String,
    },

    /// 新しいエイリアスを作成します
    Make {
        /// 反応するメッセージ（キー）
        key: String,

        /// 送信するメッセージ
        message: Option<String>,

        /// 既存のエイリアスがあったとき、上書きします
        #[clap(short, long)]
        force: bool,
    },
}

#[async_trait]
pub(crate) trait MessageAliasDatabase: Send + Sync {
    async fn save(&self, alias: MessageAlias) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Option<MessageAlias>>;
    async fn get_and_increment_usage_count(&self, key: &str) -> Result<Option<MessageAlias>>;
    async fn delete(&self, key: &str) -> Result<IsUpdated>;
    async fn len(&self) -> Result<u32>;
    async fn usage_count_top_n(&self, n: usize) -> Result<Vec<MessageAlias>>;
}

pub(crate) struct MessageAliasBot<D: MessageAliasDatabase> {
    db: D,
}

#[async_trait]
impl<D: MessageAliasDatabase> BotService for MessageAliasBot<D> {
    fn name(&self) -> &'static str {
        NAME
    }

    async fn on_message(&self, msg: &dyn Message, ctx: &dyn Context) -> Result<()> {
        if msg.content().starts_with(PREFIX) {
            if let Some(msg) = self.on_command(msg, ctx).await? {
                ctx.send_message(SendMessage {
                    content: &msg,
                    attachments: &[],
                })
                .await?;
            }
        }

        if let Some(registered_alias) = self.db.get_and_increment_usage_count(msg.content()).await?
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
    pub(crate) fn new(db: D) -> Self {
        Self { db }
    }

    async fn on_command(&self, message: &dyn Message, ctx: &dyn Context) -> Result<Option<String>> {
        use command::*;

        let Some(parsed) = parse_command::<Ui>(message.content(), ctx).await?
            else { return Ok(None) };

        match parsed.command {
            Command::Status => Ok(Some(status(&self.db).await?)),
            Command::Ranking => Ok(Some(usage_ranking(&self.db).await?)),

            Command::Delete { key } => delete(&self.db, &key).await.map(Some),

            Command::Make {
                key,
                message: text,
                force,
            } => make(
                &self.db,
                &key,
                text.as_deref(),
                message.attachments(),
                force,
            )
            .await
            .map(Some),
        }
    }
}
