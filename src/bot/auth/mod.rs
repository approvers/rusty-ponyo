use {
    crate::bot::{BotService, Context, Message, Synced, ThreadSafe},
    anyhow::{Context as _, Result},
    async_trait::async_trait,
    rand::{prelude::StdRng, Rng, SeedableRng},
    sequoia_openpgp::{
        cert::CertParser,
        parse::{PacketParser, Parse},
        policy::StandardPolicy,
        serialize::stream::{Armorer, Encryptor, LiteralWriter, Message as OpenGPGMessage},
        Cert,
    },
    sha2::Digest,
    std::{io::Write, marker::PhantomData, time::Duration},
    url::{Host, Origin, Url},
};

#[async_trait]
pub(crate) trait GenkaiAuthDatabase: ThreadSafe {
    async fn register_pgp_key(&mut self, user_id: u64, cert: &str) -> Result<()>;
    async fn get_pgp_key(&self, user_id: u64) -> Result<Option<String>>;

    async fn register_token(&mut self, user_id: u64, hashed_token: &str) -> Result<()>;
    async fn revoke_token(&mut self, user_id: u64) -> Result<()>;
    async fn get_token(&self, user_id: u64) -> Result<Option<String>>;
}

pub(crate) struct GenkaiAuthBot<D> {
    pgp_pubkey_source_domain_whitelist: Vec<String>,
    phantom: PhantomData<fn() -> D>,
}

#[async_trait]
impl<D: GenkaiAuthDatabase> BotService for GenkaiAuthBot<D> {
    const NAME: &'static str = "GenkaiAuthBot";

    type Database = D;

    async fn on_message(
        &self,
        db: &Synced<Self::Database>,
        msg: &dyn Message,
        ctx: &dyn Context,
    ) -> Result<()> {
        let tokens = msg.content().split_ascii_whitespace().collect::<Vec<_>>();

        const PREFIX: &str = "g!auth";

        match tokens.as_slice() {
            [PREFIX, "set", "pgp", url] => self.set_pgp(db, msg, ctx, url).await?,
            [PREFIX, "token"] => Self::token(db, msg, ctx).await?,
            [PREFIX, "revoke"] => Self::revoke(db, msg, ctx).await?,
            [PREFIX, ..] => Self::help(ctx).await?,

            _ => {}
        }

        Ok(())
    }
}

impl<D: GenkaiAuthDatabase> GenkaiAuthBot<D> {
    pub(crate) fn new(pubkey_whitelist: Vec<String>) -> Self {
        Self {
            pgp_pubkey_source_domain_whitelist: pubkey_whitelist,
            phantom: PhantomData,
        }
    }

    async fn help(ctx: &dyn Context) -> Result<()> {
        ctx.send_text_message(include_str!("messages/help_text.txt"))
            .await
    }

    async fn set_pgp(
        &self,
        db: &Synced<D>,
        msg: &dyn Message,
        ctx: &dyn Context,
        url: &str,
    ) -> Result<()> {
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

        db.write()
            .await
            .register_pgp_key(msg.author().id(), &cert)
            .await
            .context("failed to register gpg key")?;

        ctx.send_text_message("登録しました").await?;

        Ok(())
    }

    async fn token(db: &Synced<D>, msg: &dyn Message, ctx: &dyn Context) -> Result<()> {
        let author = msg.author();

        if db.read().await.get_token(author.id()).await?.is_some() {
            ctx.send_text_message("すでにトークンが登録されています。新しいトークンを作成したい場合は先に revoke してください。現在登録されているトークンの開示は出来ません。").await?;
            return Ok(());
        }

        let gpg_key = db
            .read()
            .await
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

        db.write()
            .await
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

    async fn revoke(db: &Synced<D>, msg: &dyn Message, ctx: &dyn Context) -> Result<()> {
        db.write()
            .await
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
