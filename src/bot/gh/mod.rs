use {
    crate::bot::{BotService, Context, Message},
    anyhow::{Context as _, Result},
    async_trait::async_trait,
    clap::{Args, CommandFactory, Parser},
    derivative::Derivative,
    once_cell::sync::Lazy,
    regex::Regex,
    reqwest::StatusCode,
    std::{fmt::Write, path::Path},
    url::Url,
};

const NAME: &str = "rusty_ponyo::bot::gh";
const PREFIX: &str = "g!gh";

/// GitHub のコードリンクからプレビューを生成します
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

    /// 指定されたリンクのプレビューを生成します。
    Preview {
        /// コードのリンク
        /// 例: https://github.com/approvers/rusty-ponyo/blob/02bb011de7d06e242a275dd9a9126a21effc6854/Cargo.toml#L48-L52
        url: String,
    },
}

#[derive(Derivative)]
#[derivative(Debug)]
enum PreviewError {
    NoUrlDetected,
    FetchError { status_code: StatusCode },
    InternalError(#[derivative(Debug = "ignore")] anyhow::Error),
}

pub struct GitHubCodePreviewBot;

impl GitHubCodePreviewBot {
    async fn on_command(&self, message: &str, ctx: &dyn Context) -> Result<()> {
        use Command::*;

        let words = match shellwords::split(message) {
            Ok(w) => w,
            Err(_) => {
                return ctx
                    .send_text_message("閉じられていない引用符があります")
                    .await
                    .context("failed to send message")
            }
        };

        let parsed = match Ui::try_parse_from(words) {
            Ok(p) => p,
            Err(e) => {
                return ctx
                    .send_text_message(&format!("```{e}```"))
                    .await
                    .context("failed to send message")
            }
        };

        match parsed.command {
            // handled by clap
            Help => Ok(()),

            Preview { url } => {
                let preview_result = self.gen_preview(&url).await;

                match preview_result {
                    Ok(ref p) => ctx.send_text_message(&p).await,
                    Err(ref e) => {
                        ctx.send_text_message(&format!("couldn't generate preview: ```{e:#?}```"))
                            .await
                    }
                }
                .context("failed to send message")?;

                if let Err(PreviewError::InternalError(e)) = preview_result {
                    return Err(e.context("failed to generate preview: internal error"));
                }

                Ok(())
            }
        }
    }

    async fn gen_preview(&self, message: &str) -> Result<String, PreviewError> {
        let links = CodePermalink::find_from_str(message);
        if links.is_empty() {
            return Err(PreviewError::NoUrlDetected);
        }

        let mut msg = String::new();

        for link in links {
            let code = link.get_code().await.map_err(|e| {
                if let Some(r) = e.downcast_ref::<reqwest::Error>() &&
                   let Some(status_code) = r.status() {
                        PreviewError::FetchError{status_code}
                    } else {
                        PreviewError::InternalError(e)
                    }
            })?;

            macro_rules! w { ($($arg:tt)*) => { let _ = writeln!(msg, $($arg)*); } }

            w!(
                "{}/{} [{}] : {}",
                link.user,
                link.repo,
                link.branch,
                link.path
            );
            w!("```{}", link.ext);
            w!("{}", code);
            w!("```");
        }

        Ok(msg)
    }
}

#[async_trait]
impl BotService for GitHubCodePreviewBot {
    fn name(&self) -> &'static str {
        NAME
    }

    async fn on_message(&self, msg: &dyn Message, ctx: &dyn Context) -> anyhow::Result<()> {
        if msg.content().starts_with(PREFIX) {
            return self.on_command(msg.content(), ctx).await;
        }

        if let Ok(msg) = self.gen_preview(msg.content()).await {
            ctx.send_text_message(&msg)
                .await
                .context("failed to send message")?;
        }

        Ok(())
    }
}

struct CodePermalink {
    user: String,
    repo: String,
    branch: String,
    path: String,
    ext: String,
    l1: usize,
    l2: Option<usize>,
}

impl CodePermalink {
    fn find_from_str(msg: &str) -> Vec<Self> {
        static URL_REGEX: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r#"https?://(?:www\.)?[-a-zA-Z0-9@:%._\+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b(?:[-a-zA-Z0-9()@:%_\+.~#?&/=]*)"#).unwrap()
        });
        static LINE_REGEX: Lazy<Regex> =
            Lazy::new(|| Regex::new(r#"L(?P<l1>\d+)(?:-L(?P<l2>\d+))?"#).unwrap());

        URL_REGEX
            .find_iter(msg)
            .flat_map(|m| {
                // e.g.: https://github.com/approvers/rusty-ponyo/blob/02bb011de7d06e242a275dd9a9126a21effc6854/Cargo.toml#L48-L52
                let url = Url::parse(m.as_str()).ok()?;

                macro_rules! bail {
                    ($e:expr) => {
                        if !$e {
                            return None;
                        }
                    };
                }

                bail!(url.host_str()? == "github.com");

                let mut segments = url.path_segments()?;
                let user = segments.next()?;
                let repo = segments.next()?;
                bail!(segments.next()? == "blob");
                let branch = segments.next()?;
                let path = segments.collect::<Vec<&str>>().join("/");

                let filename = url.path_segments()?.next_back()?;
                let ext = Path::new(filename)
                    .extension()
                    .map(|x| x.to_str().unwrap())
                    .unwrap_or("");

                let fragment = url.fragment()?; // a part after #
                let captures = LINE_REGEX.captures(fragment)?;
                let l1 = captures.name("l1").unwrap().as_str().parse().unwrap();
                let l2 = captures.name("l2").map(|x| x.as_str().parse().unwrap());

                Some(Self {
                    user: user.to_owned(),
                    repo: repo.to_owned(),
                    branch: branch.to_owned(),
                    path,
                    ext: ext.to_owned(),
                    l1,
                    l2,
                })
            })
            .collect()
    }

    async fn get_code(&self) -> Result<String> {
        let rawcode_url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            self.user, self.repo, self.branch, self.path,
        );

        let code = reqwest::get(rawcode_url)
            .await
            .context("failed to make request")?
            .error_for_status()
            .context("response had bad status code")?
            .text()
            .await
            .context("failed to download rawcode")?;

        const DEFAULT_RANGE: usize = 12;

        let (l1, l2) = match self.l2 {
            Some(l2) => (self.l1, l2),
            None => (self.l1 - DEFAULT_RANGE / 2, self.l1 + DEFAULT_RANGE / 2),
        };

        Ok(code
            .lines()
            .skip(l1 - 1)
            .take(l2 - l1)
            .collect::<Vec<&str>>()
            .join("\n"))
    }
}

#[cfg(test)]
mod test {
    use {
        super::*,
        crate::bot::{Attachment, SendMessage, User},
        pretty_assertions::assert_eq,
        std::{
            future::Future,
            pin::Pin,
            sync::atomic::{AtomicBool, Ordering},
        },
    };

    struct Msg;
    impl Message for Msg {
        fn author(&self) -> &dyn User {
            unimplemented!()
        }
        fn attachments(&self) -> &[&dyn Attachment] {
            unimplemented!()
        }
        fn content(&self) -> &str {
            r#"これはテストhttps://github.com/approvers/rusty-ponyo/blob/02bb011de7d06e242a275dd9a9126a21effc6854/Cargo.toml#L48-L52これもテストhttps://github.com/approvers/rusty-ponyo/blob/02bb011de7d06e242a275dd9a9126a21effc6854/Cargo.toml#L54"#
        }
    }

    struct Ctx {
        called: AtomicBool,
    }

    #[async_trait]
    impl Context for Ctx {
        async fn send_message(&self, _: SendMessage<'_>) -> Result<()> {
            unimplemented!()
        }

        async fn get_user_name(&self, _: u64) -> Result<String> {
            unimplemented!()
        }

        fn send_text_message<'a>(
            &'a self,
            text: &'a str,
        ) -> Pin<Box<dyn Send + Future<Output = Result<()>> + 'a>> {
            assert_eq!(
                text,
                r#"approvers/rusty-ponyo [02bb011de7d06e242a275dd9a9126a21effc6854] : Cargo.toml
```toml
version = "0.10"
optional = true
default-features = false
features = ["rustls_backend", "client", "gateway", "model", "cache"]
```
approvers/rusty-ponyo [02bb011de7d06e242a275dd9a9126a21effc6854] : Cargo.toml
```toml
version = "0.10"
optional = true
default-features = false
features = ["rustls_backend", "client", "gateway", "model", "cache"]

[dependencies.reqwest]
version = "0.11"
default-features = false
features = ["rustls-tls"]

[dependencies.tokio]
version = "1"
```
"#
            );

            self.called.store(true, Ordering::Relaxed);
            Box::pin(async { Ok(()) })
        }
    }

    #[tokio::test]
    async fn test_get_code() {
        let ctx = Ctx {
            called: AtomicBool::new(false),
        };

        GitHubCodePreviewBot.on_message(&Msg, &ctx).await.unwrap();

        assert!(ctx.called.load(Ordering::Relaxed));
    }
}
