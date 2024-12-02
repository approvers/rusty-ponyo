mod command;
pub mod model;

use {
    crate::bot::{
        alias::model::MessageAlias, parse_command, ui, BotService, Context, IsUpdated, Message,
        Runtime, SendAttachment, SendMessage,
    },
    anyhow::Result,
    std::future::Future,
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

pub trait MessageAliasDatabase: Send + Sync {
    fn save(&self, alias: MessageAlias) -> impl Future<Output = Result<()>> + Send;
    fn get(&self, key: &str) -> impl Future<Output = Result<Option<MessageAlias>>> + Send;
    fn get_and_increment_usage_count(
        &self,
        key: &str,
    ) -> impl Future<Output = Result<Option<MessageAlias>>> + Send;
    fn delete(&self, key: &str) -> impl Future<Output = Result<IsUpdated>> + Send;
    fn len(&self) -> impl Future<Output = Result<u32>> + Send;
    fn usage_count_top_n(&self, n: usize)
        -> impl Future<Output = Result<Vec<MessageAlias>>> + Send;
}

pub struct MessageAliasBot<D: MessageAliasDatabase> {
    db: D,
}

impl<R: Runtime, D: MessageAliasDatabase> BotService<R> for MessageAliasBot<D> {
    fn name(&self) -> &'static str {
        NAME
    }

    async fn on_message(&self, msg: &R::Message, ctx: &R::Context) -> Result<()> {
        if msg.content().starts_with(PREFIX) {
            if let Some(msg) = self.on_command(msg, ctx).await? {
                ctx.send_message(SendMessage {
                    content: &msg,
                    attachments: &[],
                })
                .await?;
            }
        }

        if let Some(alias) = self.db.get_and_increment_usage_count(msg.content()).await? {
            self.send_alias(ctx, &alias).await?;
        }

        Ok(())
    }
}

impl<D: MessageAliasDatabase> MessageAliasBot<D> {
    pub fn new(db: D) -> Self {
        Self { db }
    }

    async fn on_command(
        &self,
        message: &impl Message,
        ctx: &impl Context,
    ) -> Result<Option<String>> {
        let Some(parsed) = parse_command::<Ui>(message.content(), ctx).await? else {
            return Ok(None);
        };

        match parsed.command {
            Command::Status => Ok(Some(self.status().await?)),
            Command::Ranking => Ok(Some(self.usage_ranking().await?)),

            Command::Delete { key } => self.delete(&key).await.map(Some),

            Command::Make {
                key,
                message: text,
                force,
            } => {
                self.make(ctx, &key, text.as_deref(), message.attachments(), force)
                    .await?;
                Ok(None)
            }
        }
    }

    async fn send_alias(&self, ctx: &impl Context, alias: &MessageAlias) -> Result<()> {
        ctx.send_message(SendMessage {
            content: &alias.message,
            attachments: &alias
                .attachments
                .iter()
                .map(|x| SendAttachment {
                    name: &x.name,
                    data: &x.data,
                })
                .collect::<Vec<_>>(),
        })
        .await
    }
}
