use {
    crate::bot::{parse_command, ui, BotService, Context, Message},
    anyhow::{Context as _, Result},
    async_trait::async_trait,
    derivative::Derivative,
    once_cell::sync::Lazy,
    regex::Regex,
    reqwest::StatusCode,
    std::{collections::HashMap, fmt::Write, path::Path},
    url::Url,
};

const NAME: &str = "rusty_ponyo::bot::gh";
const PREFIX: &str = "g!gh";

const DEFAULT_SHOWN_LINES: usize = 12;

#[allow(clippy::identity_op)]
const DL_SIZE_LIMIT: u64 = 1 * 1024 * 1024;

ui! {
    /// GitHub のコードリンクからプレビューを生成します
    struct Ui {
        name: NAME,
        prefix: PREFIX,
        command: Command,
    }
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    /// 指定されたリンクのプレビューを生成します。
    Preview {
        #[allow(rustdoc::bare_urls)]
        /// コードのリンク
        /// 例: https://github.com/approvers/rusty-ponyo/blob/02bb011de7d06e242a275dd9a9126a21effc6854/Cargo.toml#L48-L52
        url: String,
    },
}

#[derive(Derivative)]
#[derivative(Debug)]
enum PreviewError {
    NoUrlDetected,
    Fetch { status_code: StatusCode },
    Size { expected: u64, actual: u64 },
    CodeTooLong,
    Internal(#[derivative(Debug = "ignore")] anyhow::Error),
}

impl From<anyhow::Error> for PreviewError {
    fn from(value: anyhow::Error) -> Self {
        PreviewError::Internal(value)
    }
}

pub struct GitHubCodePreviewBot;

const MAX_PREVIEW_MSG_LENGTH: usize = 2000;

impl GitHubCodePreviewBot {
    async fn on_command(&self, message: &str, ctx: &dyn Context) -> Result<()> {
        use Command::*;

        let Some(parsed) = parse_command::<Ui>(message, ctx).await? else {
            return Ok(());
        };

        match parsed.command {
            Preview { url } => {
                let preview_result = self.gen_preview(&url).await;

                match preview_result {
                    Ok(ref p) => ctx.send_text_message(p).await,
                    Err(ref e) => {
                        ctx.send_text_message(&format!("couldn't generate preview: ```{e:#?}```"))
                            .await
                    }
                }
                .context("failed to send message")?;

                if let Err(PreviewError::Internal(e)) = preview_result {
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

        let cache = CodeCache::new();
        let mut msg = String::new();
        let mut buf = String::new();

        let mut is_skipped = false;
        let mut is_backquote_replaced = false;

        for link in links {
            let code = link.get_code(&cache).await?;

            is_backquote_replaced |= code.contains("```");

            let code = code.replace("```", "'''");

            macro_rules! w { ($($arg:tt)*) => { let _ = writeln!(buf, $($arg)*); } }

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

            if msg.chars().count() + buf.chars().count() > MAX_PREVIEW_MSG_LENGTH {
                is_skipped = true;
                continue;
            }

            msg.push_str(&buf);
            buf.clear();
        }

        if msg.is_empty() {
            return Err(PreviewError::CodeTooLong);
        }

        if is_backquote_replaced {
            msg.insert_str(0, "\\`\\`\\` is replaced to '''\n");
        }

        if is_skipped {
            msg.insert_str(
                0,
                "some perma links are skipped due to message length limit.\n",
            );
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

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CodePermalink {
    user: String,
    repo: String,
    branch: String,
    path: String,
    ext: String,
    l1: usize,
    l2: Option<usize>,
}

// HashMap<Url, code>
type CodeCache = HashMap<String, String>;

impl CodePermalink {
    fn find_from_str(msg: &str) -> Vec<Self> {
        static URL_REGEX: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"https?://(?:www\.)?[-a-zA-Z0-9@:%._\+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b(?:[-a-zA-Z0-9()@:%_\+.~#?&/=]*)").unwrap()
        });
        static LINE_REGEX: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"L(?P<l1>\d+)(?:-L(?P<l2>\d+))?").unwrap());

        let mut res = URL_REGEX
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

                let l1 = captures.name("l1").unwrap().as_str().parse().ok()?;

                let l2 = match captures.name("l2") {
                    Some(l2) => Some(l2.as_str().parse().ok()?),
                    None => None,
                };

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
            .collect::<Vec<_>>();

        res.sort_unstable();
        res.dedup();
        res
    }

    async fn get_code(&self, cache: &CodeCache) -> Result<String, PreviewError> {
        let rawcode_url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            self.user, self.repo, self.branch, self.path,
        );

        if let Some(code) = cache.get(&rawcode_url) {
            return Ok(code.clone());
        }

        let res = reqwest::get(rawcode_url)
            .await
            .context("failed to make request")?;

        if !matches!(res.content_length(), Some(c) if c <= DL_SIZE_LIMIT) {
            return Err(PreviewError::Size {
                expected: DL_SIZE_LIMIT,
                actual: res.content_length().unwrap(),
            });
        }

        let res = match res.error_for_status() {
            Ok(res) => res,
            Err(code) => {
                if let Some(code) = code.status() {
                    return Err(PreviewError::Fetch { status_code: code });
                }
                Err(code).context("failed to fetch code")?
            }
        };

        let code = res.text().await.context("failed to download rawcode")?;

        const OFFSET: usize = DEFAULT_SHOWN_LINES / 2;

        let (l1, l2) = match self.l2 {
            Some(l2) => (self.l1, l2),
            None => (
                self.l1.saturating_sub(OFFSET),
                self.l1.saturating_add(OFFSET),
            ),
        };

        let skip = l1.saturating_sub(1);

        Ok(code
            .lines()
            .skip(skip)
            .take(l2.saturating_sub(skip))
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

    async fn test(input: &'static str, output: impl Into<Option<&'static str>>) {
        let output = output.into();

        struct Msg(&'static str);
        #[async_trait]
        impl Message for Msg {
            async fn reply(&self, _msg: &str) -> Result<()> {
                unimplemented!()
            }
            fn author(&self) -> &dyn User {
                unimplemented!()
            }
            fn attachments(&self) -> &[&dyn Attachment] {
                unimplemented!()
            }
            fn content(&self) -> &str {
                self.0
            }
        }

        struct Ctx {
            called: AtomicBool,
            expected: Option<&'static str>,
        }
        #[async_trait]
        impl Context for Ctx {
            async fn send_message(&self, _: SendMessage<'_>) -> Result<()> {
                unimplemented!()
            }

            async fn get_user_name(&self, _: u64) -> Result<String> {
                unimplemented!()
            }

            async fn is_bot(&self, _: u64) -> Result<bool> {
                unimplemented!()
            }

            fn send_text_message<'a>(
                &'a self,
                text: &'a str,
            ) -> Pin<Box<dyn Send + Future<Output = Result<()>> + 'a>> {
                match self.expected {
                    Some(expected) => {
                        assert_eq!(text, expected);

                        self.called.store(true, Ordering::Relaxed);
                    }

                    None => panic!("should never send message, but it actually sent."),
                }
                Box::pin(async { Ok(()) })
            }
        }

        let ctx = Ctx {
            called: AtomicBool::new(false),
            expected: output,
        };

        GitHubCodePreviewBot
            .on_message(&Msg(input), &ctx)
            .await
            .unwrap();

        assert!(output.is_some() == ctx.called.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_get_code() {
        test(
            r#"これはテストhttps://github.com/approvers/rusty-ponyo/blob/02bb011de7d06e242a275dd9a9126a21effc6854/Cargo.toml#L48-L52これもテストhttps://github.com/approvers/rusty-ponyo/blob/02bb011de7d06e242a275dd9a9126a21effc6854/Cargo.toml#L54"#,

r#"approvers/rusty-ponyo [02bb011de7d06e242a275dd9a9126a21effc6854] : Cargo.toml
```toml
[dependencies.serenity]
version = "0.10"
optional = true
default-features = false
features = ["rustls_backend", "client", "gateway", "model", "cache"]
```
approvers/rusty-ponyo [02bb011de7d06e242a275dd9a9126a21effc6854] : Cargo.toml
```toml
[dependencies.serenity]
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
"#).await
    }

    #[tokio::test]
    async fn test_backquote() {
        test(
            r#"https://github.com/approvers/rusty-ponyo/blob/5793b96bbdbb75b008a1a02a07f64081f4219242/src/bot/gh/mod.rs#L81"#,
r#"\`\`\` is replaced to '''
approvers/rusty-ponyo [5793b96bbdbb75b008a1a02a07f64081f4219242] : src/bot/gh/mod.rs
```rs
        };

        let parsed = match Ui::try_parse_from(words) {
            Ok(p) => p,
            Err(e) => {
                return ctx
                    .send_text_message(&format!("'''{e}'''"))
                    .await
                    .context("failed to send message")
            }
        };

        match parsed.command {
```
"#,).await
    }

    #[tokio::test]
    async fn test_long() {
        test(
r#"https://github.com/approvers/rusty-ponyo/blob/765314882920494fb48c72a33d747d96a39bc23f/Cargo.lock#L1
https://github.com/approvers/rusty-ponyo/blob/765314882920494fb48c72a33d747d96a39bc23f/Cargo.lock#L5-L426
"#,

r#"some perma links are skipped due to message length limit.
approvers/rusty-ponyo [765314882920494fb48c72a33d747d96a39bc23f] : Cargo.lock
```lock
# This file is automatically @generated by Cargo.
# It is not intended for manual editing.
version = 3

[[package]]
name = "adler"
version = "1.0.2"
```
"#).await
    }

    #[tokio::test]
    async fn test_long2() {
        test(
            r#"https://github.com/approvers/rusty-ponyo/blob/master/Cargo.lock#L1-L2712"#,
            None,
        )
        .await
    }
}
