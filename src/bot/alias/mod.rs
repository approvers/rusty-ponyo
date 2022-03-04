use super::SendAttachment;

mod command;
pub(crate) mod model;

use {
    crate::{
        bot::{alias::model::MessageAlias, BotService, Context, Message, SendMessage},
        Synced, ThreadSafe,
    },
    anyhow::Result,
    async_trait::async_trait,
    clap::{Args, CommandFactory, Parser},
    std::marker::PhantomData,
};

const NAME: &str = "rusty_ponyo::bot::alias";
const PREFIX: &str = "g!alias";

/// 特定のメッセージが送信されたときに、指定されたメッセージを同じ場所に送信します。
#[derive(Debug, clap::Args)]
#[clap(name=NAME, about, long_about=None)]
struct Ui {
    #[clap(subcommand)]
    command: Command,
}

impl Ui {
    fn command<'a>() -> clap::Command<'a> {
        clap::Command::new(NAME).bin_name(PREFIX)
    }
}
impl Parser for Ui {}
impl CommandFactory for Ui {
    fn into_app<'help>() -> clap::Command<'help> {
        Self::augment_args(Self::command())
    }
    fn into_app_for_update<'help>() -> clap::Command<'help> {
        Self::augment_args_for_update(Self::command())
    }
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    /// ヘルプメッセージを出します
    Help,

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
    },
}

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
    const NAME: &'static str = NAME;
    type Database = D;

    async fn on_message(
        &self,
        db: &Synced<Self::Database>,
        msg: &dyn Message,
        ctx: &dyn Context,
    ) -> Result<()> {
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
        use command::*;

        let words = match shellwords::split(message.content()) {
            Ok(w) => w,
            Err(_) => return Ok(Some("閉じられていない引用符があります".to_string())),
        };

        let parsed = match Ui::try_parse_from(words) {
            Ok(p) => p,
            Err(e) => return Ok(Some(format!("```{e}```"))),
        };

        match parsed.command {
            // help command should be handled automatically by clap
            Command::Help => Ok(None),
            Command::Status => Ok(Some(status(db).await?)),
            Command::Ranking => Ok(Some(usage_ranking(db).await?)),

            Command::Delete { key } => delete(db, &key).await.map(Some),

            Command::Make { key, message: text } => {
                make(db, &key, text.as_deref(), message.attachments())
                    .await
                    .map(Some)
            }
        }
    }
}
