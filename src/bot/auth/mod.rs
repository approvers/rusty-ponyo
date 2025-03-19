use {
    crate::bot::{BotService, Context, Message, Runtime, User, parse_command, ui},
    anyhow::{Context as _, Result},
    rand::{Rng, SeedableRng, prelude::StdRng},
    sequoia_openpgp::{
        Cert,
        cert::CertParser,
        parse::{PacketParser, Parse},
        policy::StandardPolicy,
        serialize::stream::{Armorer, Encryptor2, LiteralWriter, Message as OpenGPGMessage},
    },
    sha2::Digest,
    std::{future::Future, io::Write, time::Duration},
    url::{Host, Origin, Url},
};

const NAME: &str = "rusty_ponyo::bot::auth";
const PREFIX: &str = "g!auth";

ui! {
    /// 限界認証情報の設定管理を行います
    struct Ui {
        name: NAME,
        prefix: PREFIX,
        command: Command,
    }
}

#[derive(Debug, clap::Subcommand)]
enum Command {
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

pub trait GenkaiAuthDatabase: Send + Sync {
    fn register_pgp_key(&self, user_id: u64, cert: &str)
    -> impl Future<Output = Result<()>> + Send;
    fn get_pgp_key(&self, user_id: u64) -> impl Future<Output = Result<Option<String>>> + Send;

    fn register_token(
        &self,
        user_id: u64,
        hashed_token: &str,
    ) -> impl Future<Output = Result<()>> + Send;
    fn revoke_token(&self, user_id: u64) -> impl Future<Output = Result<()>> + Send;
    fn get_token(&self, user_id: u64) -> impl Future<Output = Result<Option<String>>> + Send;
}

pub struct GenkaiAuthBot<D> {
    db: D,
    pgp_pubkey_source_domain_whitelist: Vec<String>,
}

impl<R: Runtime, D: GenkaiAuthDatabase> BotService<R> for GenkaiAuthBot<D> {
    fn name(&self) -> &'static str {
        NAME
    }

    async fn on_message(&self, msg: &R::Message, ctx: &R::Context) -> Result<()> {
        if !msg.content().starts_with(PREFIX) {
            return Ok(());
        }

        let Some(parsed) = parse_command::<Ui>(msg.content(), ctx).await? else {
            return Ok(());
        };

        match parsed.command {
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
    pub fn new(db: D, pubkey_whitelist: Vec<String>) -> Self {
        Self {
            db,
            pgp_pubkey_source_domain_whitelist: pubkey_whitelist,
        }
    }

    async fn set_pgp(&self, msg: &impl Message, ctx: &impl Context, url: &str) -> Result<()> {
        let verify_result = match self.verify_url(url) {
            Ok(_) => download_gpg_key(url).await,
            Err(e) => Err(e),
        };

        let cert = match verify_result {
            Ok(v) => v,
            Err(e) => {
                ctx.send_text_message(&format!(
                    "公開鍵の処理に失敗しました。URLを確認して下さい。: {e}",
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

    async fn token(&self, msg: &impl Message, ctx: &impl Context) -> Result<()> {
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

        let mut token = gen_token();

        let mut hasher = sha2::Sha512::new();
        hasher.update(token.as_bytes());
        let hashed = hasher.finalize();
        let hashed = hex::encode(hashed);

        self.db
            .register_token(msg.author().id(), &hashed)
            .await
            .context("failed to register new token")?;

        token.push('\n');
        let encrypted_token = encrypt(&gpg_key.unwrap(), &token)?;

        author
            .dm_text(&format!(
                include_str!("messages/token_text.txt"),
                TOKEN = encrypted_token
            ))
            .await?;

        Ok(())
    }

    async fn revoke(&self, msg: &impl Message, ctx: &impl Context) -> Result<()> {
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
    const DL_SIZE_LIMIT: u64 = 1024 * 64; // 64KIB

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
    let message = Encryptor2::for_recipients(message, recipients)
        .build()
        .unwrap();
    let mut message = LiteralWriter::new(message).build().unwrap();
    message.write_all(text.as_bytes()).unwrap();
    message.finalize().unwrap();

    Ok(String::from_utf8_lossy(&output).into_owned())
}
