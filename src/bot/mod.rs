use {
    anyhow::{Context as _, Result},
    std::future::Future,
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

// Usage of GATs like this:
// type Message<'a>: Message + 'a;
// makes hard lifetime error
// repro: https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=97640973cf3459848463dbd13ba8f951
// issue: https://github.com/rust-lang/rust/issues/100013

pub trait Message: Send + Sync {
    type Attachment: Attachment;
    type User: User;

    fn reply(&self, msg: &str) -> impl Future<Output = Result<()>> + Send;
    fn author(&self) -> &Self::User;
    fn content(&self) -> &str;
    fn attachments(&self) -> &[Self::Attachment];
}

pub trait Attachment: Send + Sync {
    fn name(&self) -> &str;
    fn size(&self) -> usize;
    fn download(&self) -> impl Future<Output = Result<Vec<u8>>> + Send;
}

pub trait User: Send + Sync {
    fn id(&self) -> u64;
    fn name(&self) -> &str;
    fn dm(&self, msg: SendMessage<'_>) -> impl Future<Output = Result<()>> + Send;

    fn dm_text(&self, text: &str) -> impl Future<Output = Result<()>> + Send {
        async move {
            self.dm(SendMessage {
                content: text,
                attachments: &[],
            })
            .await
        }
    }
}

pub struct SendMessage<'a> {
    pub content: &'a str,
    pub attachments: &'a [SendAttachment<'a>],
}

pub struct SendAttachment<'a> {
    pub name: &'a str,
    #[allow(dead_code)]
    pub data: &'a [u8],
}

pub trait Context: Send + Sync {
    fn send_message(&self, msg: SendMessage<'_>) -> impl Future<Output = Result<()>> + Send;
    fn get_user_name(&self, user_id: u64) -> impl Future<Output = Result<String>> + Send;
    fn is_bot(&self, user_id: u64) -> impl Future<Output = Result<bool>> + Send;

    fn send_text_message(&self, text: &str) -> impl Future<Output = Result<()>> + Send {
        async move {
            self.send_message(SendMessage {
                content: text,
                attachments: &[],
            })
            .await
        }
    }
}

pub trait Runtime {
    type Message: Message;
    type Context: Context;
}

pub trait BotService<R: Runtime>: Send + Sync {
    fn name(&self) -> &'static str;

    fn on_message(
        &self,
        _msg: &R::Message,
        _ctx: &R::Context,
    ) -> impl Future<Output = Result<()>> + Send {
        async { Ok(()) }
    }

    // called on bot started and got who is currently joined to vc
    fn on_vc_data_available(
        &self,
        _ctx: &R::Context,
        _joined_user_ids: &[u64],
    ) -> impl Future<Output = Result<()>> + Send {
        async { Ok(()) }
    }

    // called on user has joined to vc
    fn on_vc_join(
        &self,
        _ctx: &R::Context,
        _user_id: u64,
    ) -> impl Future<Output = Result<()>> + Send {
        async { Ok(()) }
    }

    // called on user has left from vc and not in any channel
    fn on_vc_leave(
        &self,
        _ctx: &R::Context,
        _user_id: u64,
    ) -> impl Future<Output = Result<()>> + Send {
        async { Ok(()) }
    }
}

async fn parse_command<Ui: clap::Parser>(message: &str, ctx: &impl Context) -> Result<Option<Ui>> {
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
