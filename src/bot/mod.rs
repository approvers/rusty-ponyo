use {
    anyhow::{Context as _, Result},
    async_trait::async_trait,
    std::{future::Future, pin::Pin},
};

/// 変更が生じた場合 true
pub type IsUpdated = bool;

pub mod alias;
pub mod auth;
pub mod genkai_point;
pub mod gh;
pub mod meigen;
pub mod uo;
pub mod vc_diff;

#[async_trait]
pub(crate) trait Message: Send + Sync {
    async fn reply(&self, msg: &str) -> Result<()>;
    fn author(&self) -> &dyn User;
    fn content(&self) -> &str;
    fn attachments(&self) -> &[&dyn Attachment];
}

#[async_trait]
pub(crate) trait Attachment: Send + Sync {
    fn name(&self) -> &str;
    fn size(&self) -> usize;
    async fn download(&self) -> Result<Vec<u8>>;
}

#[async_trait]
pub(crate) trait User: Send + Sync {
    fn id(&self) -> u64;
    #[allow(unused)]
    fn name(&self) -> &str;
    async fn dm(&self, msg: SendMessage<'_>) -> Result<()>;

    fn dm_text<'a>(
        &'a self,
        text: &'a str,
    ) -> Pin<Box<dyn Send + Future<Output = Result<()>> + 'a>> {
        self.dm(SendMessage {
            content: text,
            attachments: &[],
        })
    }
}

pub(crate) struct SendMessage<'a> {
    pub(crate) content: &'a str,
    pub(crate) attachments: &'a [SendAttachment<'a>],
}

pub(crate) struct SendAttachment<'a> {
    pub(crate) name: &'a str,
    #[allow(dead_code)]
    pub(crate) data: &'a [u8],
}

#[async_trait]
pub(crate) trait Context: Send + Sync {
    async fn send_message(&self, msg: SendMessage<'_>) -> Result<()>;
    async fn get_user_name(&self, user_id: u64) -> Result<String>;
    async fn is_bot(&self, user_id: u64) -> Result<bool>;

    #[must_use = "Futures do nothing unless polled"]
    fn send_text_message<'a>(
        &'a self,
        text: &'a str,
    ) -> Pin<Box<dyn Send + Future<Output = Result<()>> + 'a>> {
        self.send_message(SendMessage {
            content: text,
            attachments: &[],
        })
    }
}

#[async_trait]
pub(crate) trait BotService: Send + Sync {
    fn name(&self) -> &'static str;

    async fn on_message(&self, _msg: &dyn Message, _ctx: &dyn Context) -> Result<()> {
        Ok(())
    }

    // called on bot started and got who is currently joined to vc
    async fn on_vc_data_available(
        &self,
        _ctx: &dyn Context,
        _joined_user_ids: &[u64],
    ) -> Result<()> {
        Ok(())
    }

    // called on user has joined to vc
    async fn on_vc_join(&self, _ctx: &dyn Context, _user_id: u64) -> Result<()> {
        Ok(())
    }

    // called on user has left from vc and not in any channel
    async fn on_vc_leave(&self, _ctx: &dyn Context, _user_id: u64) -> Result<()> {
        Ok(())
    }
}

async fn parse_command<Ui: clap::Parser>(message: &str, ctx: &dyn Context) -> Result<Option<Ui>> {
    let words = match shellwords::split(message) {
        Ok(w) => w,
        Err(_) => {
            ctx.send_text_message("閉じられていない引用符があります")
                .await
                .context("failed to send message")?;
            return Ok(None);
        }
    };

    let parsed = match Ui::try_parse_from(words) {
        Ok(p) => p,
        Err(e) => {
            ctx.send_text_message(&format!("```{e}```"))
                .await
                .context("failed to send message")?;
            return Ok(None);
        }
    };

    Ok(Some(parsed))
}

macro_rules! ui {
    (
        $(#[$meta:meta])*
        struct $name:ident {
            name: $bot_name:ident,
            prefix: $prefix:ident,
            command: $command:ident,
            $($rest:tt)*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, clap::Args)]
        #[clap(name=$bot_name, about, version, long_about=None)]
        struct $name {
            #[clap(subcommand)]
            command: $command,
            $($rest)*
        }

        impl $name {
            fn command() -> clap::Command {
                clap::Command::new($bot_name).bin_name($prefix)
            }
        }
        impl clap::Parser for $name {}
        impl clap::CommandFactory for $name {
            fn command() -> clap::Command {
                use clap::Args;
                Self::augment_args(Self::command())
            }
            fn command_for_update() -> clap::Command {
                use clap::Args;
                Self::augment_args_for_update(Self::command())
            }
        }
    };
}
use ui;
