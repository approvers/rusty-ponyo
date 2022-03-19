use {
    crate::bot::{BotService, Context, Message},
    anyhow::{Context as _, Result},
    async_trait::async_trait,
    clap::{Args, CommandFactory, Parser},
    rand::{prelude::StdRng, Rng, SeedableRng},
    sequoia_openpgp::{
        cert::CertParser,
        parse::{PacketParser, Parse},
        policy::StandardPolicy,
        serialize::stream::{Armorer, Encryptor, LiteralWriter, Message as OpenGPGMessage},
        Cert,
    },
    sha2::Digest,
    std::{io::Write, time::Duration},
    url::{Host, Origin, Url},
};

const NAME: &str = "rusty_ponyo::bot::auth";
const PREFIX: &str = "g!auth";

/// 限界認証情報の設定管理を行います
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

    Set {
        #[clap(subcommand)]
        what: SetCommand,
    },

    /// あなたのトークンを作成してDMに送信します
    Token,

    /// あなたのトークンを無効化します
    Revoke,
}

#[derive(Debug, clap::Subcommand)]
enum SetCommand {
    /// PGP公開鍵を設定します
    Pgp {
        /// 公開鍵のURL
        src_url: String,
    },
}

#[async_trait]
pub(crate) trait GenkaiAuthDatabase: Send + Sync {
    async fn register_pgp_key(&self, user_id: u64, cert: &str) -> Result<()>;
    async fn get_pgp_key(&self, user_id: u64) -> Result<Option<String>>;

    async fn register_token(&self, user_id: u64, hashed_token: &str) -> Result<()>;
    async fn revoke_token(&self, user_id: u64) -> Result<()>;
    async fn get_token(&self, user_id: u64) -> Result<Option<String>>;
}

pub(crate) struct GenkaiAuthBot<D> {
    db: D,
    pgp_pubkey_source_domain_whitelist: Vec<String>,
}

#[async_trait]
impl<D: GenkaiAuthDatabase> BotService for GenkaiAuthBot<D> {
    fn name(&self) -> &'static str {
        NAME
    }

    async fn on_message(&self, msg: &dyn Message, ctx: &dyn Context) -> Result<()> {
        if !msg.content().starts_with(PREFIX) {
            return Ok(());
        }

        let words = match shellwords::split(msg.content()) {
            Ok(w) => w,
            Err(_) => {
                return ctx
                    .send_text_message("閉じられていない引用符があります")
                    .await
            }
        };

        let parsed = match Ui::try_parse_from(words) {
            Ok(p) => p,
            Err(e) => return ctx.send_text_message(&format!("```{e}```")).await,
        };

        match parsed.command {
            // help command should be handled automatically by clap
            Command::Help => {}

            Command::Set {
                what: SetCommand::Pgp { src_url },
            } => self.set_pgp(msg, ctx, &src_url).await?,
            Command::Token => self.token(msg, ctx).await?,
            Command::Revoke => self.revoke(msg, ctx).await?,
        }

        Ok(())
    }
}

impl<D: GenkaiAuthDatabase> GenkaiAuthBot<D> {
    pub(crate) fn new(db: D, pubkey_whitelist: Vec<String>) -> Self {
        Self {
            db,
            pgp_pubkey_source_domain_whitelist: pubkey_whitelist,
        }
    }

    async fn set_pgp(&self, msg: &dyn Message, ctx: &dyn Context, url: &str) -> Result<()> {
        let verify_result = match self.verify_url(url) {
            Ok(_) => download_gpg_key(url).await,
            Err(e) => Err(e),
        };

        let cert = match verify_result {
            Ok(v) => v,
            Err(e) => {
                ctx.send_text_message(&format!(
                    "公開鍵の処理に失敗しました。URLを確認して下さい。: {}",
                    e
                ))
                .await?;
                return Ok(());
            }
        };

        self.db
            .register_pgp_key(msg.author().id(), &cert)
            .await
            .context("failed to register gpg key")?;

        ctx.send_text_message("登録しました").await?;

        Ok(())
    }

    async fn token(&self, msg: &dyn Message, ctx: &dyn Context) -> Result<()> {
        let author = msg.author();

        if self.db.get_token(author.id()).await?.is_some() {
            ctx.send_text_message("すでにトークンが登録されています。新しいトークンを作成したい場合は先に revoke してください。現在登録されているトークンの開示は出来ません。").await?;
            return Ok(());
        }

        let gpg_key = self
            .db
            .get_pgp_key(msg.author().id())
            .await
            .context("failed to fetch user's gpg key")?;

        if gpg_key.is_none() {
            ctx.send_text_message("GPG鍵が登録されていません。トークンを送信するために必要です。登録方法はhelpを参照してください。").await?;
            return Ok(());
        }

        let generated_token = gen_token();

        let mut hasher = sha2::Sha512::new();
        hasher.update(generated_token.as_bytes());
        let hashed = hasher.finalize();
        let hashed = hex::encode(&hashed);

        self.db
            .register_token(msg.author().id(), &hashed)
            .await
            .context("failed to register new token")?;

        let token = Some(generated_token);

        let mut token = token.unwrap();
        token.push('\n');
        let token = encrypt(&gpg_key.unwrap(), &token)?;

        author
            .dm_text(&format!(
                include_str!("messages/token_text.txt"),
                TOKEN = token
            ))
            .await?;

        Ok(())
    }

    async fn revoke(&self, msg: &dyn Message, ctx: &dyn Context) -> Result<()> {
        self.db
            .revoke_token(msg.author().id())
            .await
            .context("failed to revoke token")?;

        ctx.send_text_message("トークンを無効化しました。").await?;

        Ok(())
    }

    fn verify_url(&self, url: &str) -> std::result::Result<(), &'static str> {
        let url = Url::parse(url).map_err(|_| "failed to parse url")?;

        if !matches!(url.origin(), Origin::Tuple(_, Host::Domain(d), _)
                if self.pgp_pubkey_source_domain_whitelist.contains(&d))
        {
            return Err("provided url doesn't contain domain or its domain is not on whitelist.");
        }

        Ok(())
    }
}

fn gen_token() -> String {
    const PREFIX: &str = "gauth";
    const LEN: usize = 80;
    const BLUR: usize = 10;

    let rng = rand::thread_rng();
    let mut rng = StdRng::from_rng(rng).expect("failed to initialize rng");
    let rune = |rng: &mut StdRng| rng.gen_range(33u8..=117) as char;

    let mut token = String::with_capacity(PREFIX.len() + LEN + BLUR + 1);

    token.push_str(PREFIX);

    for _ in 0..LEN {
        token.push(rune(&mut rng));
    }

    for _ in 0..rng.gen_range(0..BLUR) {
        token.push(rune(&mut rng));
    }

    token
}

fn parse_gpg_key(armored: &str) -> Result<Vec<Cert>> {
    let parser =
        PacketParser::from_bytes(armored.as_bytes()).context("failed to parse key(packet)")?;

    CertParser::from(parser)
        .collect::<Result<Vec<Cert>, _>>()
        .context("failed to parse key(cert)")
}

// verify that certs are parsable, and return downloaded armor certs.
async fn download_gpg_key(url: &str) -> std::result::Result<String, &'static str> {
    const TIMEOUT: Duration = Duration::from_secs(5);
    const DL_SIZE_LIMIT: u64 = 1024 * 10; // 10KIB

    let client = reqwest::Client::builder()
        .connect_timeout(TIMEOUT)
        .timeout(TIMEOUT)
        .build()
        .expect("failed to build reqwest client");

    let con = client
        .get(url)
        .send()
        .await
        .map_err(|_| "failed to connect to host")?;

    match con.content_length() {
        Some(size) if size <= DL_SIZE_LIMIT => {}
        _ => return Err("download size is too big or not known"),
    }

    let body = con
        .text()
        .await
        .map_err(|_| "failed to download key(body)")?;

    let _ = parse_gpg_key(&body).map_err(|_| "failed to parse key")?;

    Ok(body)
}

fn encrypt(cert: &str, text: &str) -> Result<String> {
    let certs = parse_gpg_key(cert)?;
    let policy = StandardPolicy::new();

    let recipients = certs.iter().flat_map(|x| {
        x.keys()
            .with_policy(&policy, None)
            .supported()
            .alive()
            .revoked(false)
            .for_transport_encryption()
    });

    let mut output = vec![];
    let message = OpenGPGMessage::new(&mut output);
    let message = Armorer::new(message).build().unwrap();
    let message = Encryptor::for_recipients(message, recipients)
        .build()
        .unwrap();
    let mut message = LiteralWriter::new(message).build().unwrap();
    message.write_all(text.as_bytes()).unwrap();
    message.finalize().unwrap();

    Ok(String::from_utf8_lossy(&output).into_owned())
}
