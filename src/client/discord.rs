use {
    crate::{
        bot::{Attachment, BotService, Context, Message, SendMessage, User},
        client::{ServiceEntry, ServiceEntryInner},
        Synced, ThreadSafe,
    },
    anyhow::{Context as _, Result},
    async_trait::async_trait,
    hashbrown::{HashMap, HashSet},
    once_cell::sync::Lazy,
    serenity::{
        http::AttachmentType,
        model::{
            channel::{Attachment as SerenityAttachment, Message as SerenityMessage},
            gateway::Ready,
            id::{
                ChannelId as SerenityChannelId, GuildId as SerenityGuildId,
                UserId as SerenityUserId,
            },
            voice::VoiceState,
        },
        prelude::{Client, Context as SerenityContext, EventHandler},
    },
    std::{future::Future, pin::Pin, sync::Arc, time::Duration},
    tokio::{
        sync::{Mutex, RwLock},
        time::interval,
    },
};

pub(crate) struct DiscordClient {
    services: Vec<Box<dyn ServiceEntry>>,
}

impl DiscordClient {
    pub fn new() -> Self {
        Self { services: vec![] }
    }

    pub fn add_service<S, D>(&mut self, service: S, db: Synced<D>) -> &mut Self
    where
        S: BotService<Database = D> + 'static,
        D: ThreadSafe + 'static,
    {
        self.services
            .push(Box::new(ServiceEntryInner { service, db }));
        self
    }

    pub async fn run(self, token: &str) -> Result<()> {
        let event_handler = EvHandler::new(self.services);

        Client::builder(token)
            .event_handler(event_handler)
            .await
            .context("Failed to create Discord client")?
            .start()
            .await
            .context("Failed to start Discord client")
    }
}

// TODO: should be configurable
const APPROVERS_GUILD_ID: u64 = 683939861539192860;
const APPROVERS_DEFAULT_CHANNEL_ID: u64 = 690909527461199922;

struct EvHandlerInner {
    services: Vec<Box<dyn ServiceEntry>>,
    vc_joined_users: Mutex<HashSet<SerenityUserId>>,
}

struct EvHandler {
    inner: Arc<EvHandlerInner>,
}

impl EvHandler {
    fn new(services: Vec<Box<dyn ServiceEntry>>) -> Self {
        Self {
            inner: Arc::new(EvHandlerInner {
                services,
                vc_joined_users: Mutex::new(HashSet::new()),
            }),
        }
    }

    // TODO: async closure will make this function more comfortable.
    async fn do_for_each_service<'a, F>(
        ctx: &'a SerenityContext,
        inner: &'a EvHandlerInner,
        op: &'static str,
        f: F,
    ) where
        F: Fn(&'a dyn ServiceEntry) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
            + Send
            + Sync
            + 'a,
    {
        for service in &inner.services {
            let result: Result<()> = f(service.as_ref()).await;

            if let Err(e) = result {
                tracing::error!(
                    "Service({})::{} returned error: {:?}",
                    service.name(),
                    op,
                    e
                );

                let _ = SerenityChannelId(APPROVERS_DEFAULT_CHANNEL_ID)
                    .say(
                        &ctx,
                        "unexpected error reported. see log <@!391857452360007680>",
                    )
                    .await;
            }
        }
    }

    async fn hoge(inner: Arc<EvHandlerInner>, ctx: SerenityContext) {
        let mut interval = interval(Duration::from_secs(1));

        loop {
            interval.tick().await;

            let guild = match ctx.cache.guild(APPROVERS_GUILD_ID).await {
                Some(g) => g,
                None => continue,
            };

            let joined_users = guild
                .voice_states
                .iter()
                .map(|(user_id, _)| user_id.0)
                .collect::<Vec<_>>();

            for user_id in &joined_users {
                tracing::info!("joined users on startup: {}", user_id);

                inner
                    .vc_joined_users
                    .lock()
                    .await
                    .insert(SerenityUserId(*user_id));
            }

            let converted_ctx = DiscordContext::from_serenity(
                &ctx,
                APPROVERS_DEFAULT_CHANNEL_ID,
                Some(APPROVERS_GUILD_ID),
            );

            Self::do_for_each_service(&ctx, &inner, "on_vc_data_available", |s| {
                Box::pin(s.on_vc_data_available(&converted_ctx, &joined_users))
            })
            .await;

            tracing::info!("vc status checking on startup complete");
            break;
        }

        tokio::spawn(Self::validate_vc_cache_loop(inner, ctx));
    }

    async fn validate_vc_cache_loop(inner: Arc<EvHandlerInner>, ctx: SerenityContext) {
        let mut interval = interval(Duration::from_secs(30));

        loop {
            interval.tick().await;

            let guild = match ctx.cache.guild(APPROVERS_GUILD_ID).await {
                Some(g) => g,
                None => {
                    tracing::warn!("missing guild in validate_vc_cache_loop. This is not good sign because mismatch of inner.vc_state can occur.");
                    continue;
                }
            };

            let converted_ctx = DiscordContext::from_serenity(
                &ctx,
                APPROVERS_DEFAULT_CHANNEL_ID,
                Some(APPROVERS_GUILD_ID),
            );

            let mut self_state = inner.vc_joined_users.lock().await;
            let serenity_state = &guild.voice_states;

            let missing_in_self_state = serenity_state
                .iter()
                .map(|(user_id, _)| user_id)
                .filter(|x| !self_state.contains(x))
                .cloned()
                .collect::<Vec<_>>();

            let missing_in_serenity_state = self_state
                .iter()
                .filter(|x| !serenity_state.contains_key(x))
                .cloned()
                .collect::<Vec<_>>();

            for uid in missing_in_self_state {
                self_state.insert(uid);
                tracing::info!("user({}) has actually joined to vc", uid.0);

                Self::do_for_each_service(&ctx, &inner, "on_vc_join", |s| {
                    Box::pin(s.on_vc_join(&converted_ctx, uid.0))
                })
                .await;
            }

            for uid in missing_in_serenity_state {
                tracing::info!("user({}) has actually left from vc", uid.0);

                self_state.remove(&uid);

                Self::do_for_each_service(&ctx, &inner, "on_vc_leave", |s| {
                    Box::pin(s.on_vc_leave(&converted_ctx, uid.0))
                })
                .await;
            }
        }
    }
}

#[async_trait]
impl EventHandler for EvHandler {
    async fn ready(&self, ctx: SerenityContext, ready: Ready) {
        tracing::info!("DiscordBot({}) is connected!", ready.user.name);

        let inner = Arc::clone(&self.inner);
        tokio::spawn(Self::hoge(inner, ctx));
    }

    async fn voice_state_update(
        &self,
        ctx: SerenityContext,
        gid: Option<SerenityGuildId>,
        _: Option<VoiceState>,
        state: VoiceState,
    ) {
        let is_approvers_event = gid.map(|x| x == APPROVERS_GUILD_ID).unwrap_or(false);
        if !is_approvers_event {
            return;
        }

        let user_id = state.user_id;
        let currently_joined = state.channel_id.is_some();

        let mut self_state = self.inner.vc_joined_users.lock().await;

        let self_state_currently_joined = self_state.iter().any(|x| *x == user_id);

        let converted_ctx = DiscordContext::from_serenity(&ctx, APPROVERS_DEFAULT_CHANNEL_ID, gid);

        match (currently_joined, self_state_currently_joined) {
            // joined
            (true, false) => {
                tracing::debug!("User({}) has joined to vc", user_id.0,);

                self_state.insert(user_id);

                Self::do_for_each_service(&ctx, &self.inner, "on_vc_join", |s| {
                    Box::pin(s.on_vc_join(&converted_ctx, user_id.0))
                })
                .await;
            }

            // left
            (false, true) => {
                tracing::debug!("User({}) has left from vc", user_id.0);

                self_state.remove(&user_id);

                Self::do_for_each_service(&ctx, &self.inner, "on_vc_leave", |s| {
                    Box::pin(s.on_vc_leave(&converted_ctx, user_id.0))
                })
                .await;
            }

            // moved to other channel or something
            (true, true) => {}

            // ???
            (false, false) => {}
        };
    }

    async fn message(&self, ctx: SerenityContext, message: SerenityMessage) {
        if message.author.bot {
            return;
        }

        let converted_attachments = message
            .attachments
            .iter()
            .map(DiscordAttachment)
            .collect::<Vec<_>>();

        let converted_message = DiscordMessage {
            content: message.content.clone(),
            attachments: converted_attachments.iter().map(|x| x as _).collect(),
            author: DiscordAuthor {
                id: message.author.id.0,
                name: message
                    .author_nick(&ctx)
                    .await
                    .unwrap_or(message.author.name),
            },
        };

        let converted_context =
            DiscordContext::from_serenity(&ctx, message.channel_id, message.guild_id);

        for service in &self.inner.services {
            let result = service
                .on_message(&converted_message, &converted_context)
                .await;

            if let Err(err) = result {
                tracing::error!(
                    "Service {} on_message returned error : {:?}",
                    service.name(),
                    err
                );
            }
        }
    }
}

struct DiscordMessage<'a> {
    content: String,
    attachments: Vec<&'a dyn Attachment>,
    author: DiscordAuthor,
}

impl Message for DiscordMessage<'_> {
    fn content(&self) -> &str {
        &self.content
    }

    fn attachments(&self) -> &[&dyn Attachment] {
        &self.attachments
    }

    fn author(&self) -> &dyn User {
        &self.author
    }
}

struct DiscordAuthor {
    id: u64,
    name: String,
}

impl User for DiscordAuthor {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }
}

struct DiscordAttachment<'a>(&'a SerenityAttachment);

#[async_trait]
impl Attachment for DiscordAttachment<'_> {
    fn name(&self) -> &str {
        &self.0.filename
    }

    async fn download(&self) -> Result<Vec<u8>> {
        self.0
            .download()
            .await
            .context("failed to download attachment from discord")
    }
}

#[derive(Clone)]
struct DiscordContext {
    origin: SerenityContext,
    channel_id: SerenityChannelId,
    guild_id: Option<SerenityGuildId>,
}

impl DiscordContext {
    fn from_serenity(
        origin: &SerenityContext,
        channel_id: impl Into<SerenityChannelId>,
        guild_id: Option<impl Into<SerenityGuildId>>,
    ) -> Self {
        Self {
            origin: origin.clone(),
            channel_id: channel_id.into(),
            guild_id: guild_id.map(|x| x.into()),
        }
    }
}

#[async_trait]
impl Context for DiscordContext {
    async fn send_message(&self, msg: SendMessage<'_>) -> Result<()> {
        let files = msg
            .attachments
            .iter()
            .map(|x| AttachmentType::Bytes {
                data: x.data.into(),
                filename: x.name.to_string(),
            })
            .collect::<Vec<_>>();

        self.channel_id
            .send_files(&self.origin.http, files, |m| m.content(msg.content))
            .await
            .context("failed to send message to discord")?;

        Ok(())
    }

    async fn get_user_name(&self, user_id: u64) -> Result<String> {
        static CACHE: Lazy<RwLock<HashMap<(Option<SerenityGuildId>, SerenityUserId), String>>> =
            Lazy::new(|| RwLock::new(HashMap::new()));

        let user_id = SerenityUserId(user_id);

        let hit = CACHE.read().await.get(&(self.guild_id, user_id)).cloned();

        if let Some(hit) = hit {
            return Ok(hit);
        }

        let user = user_id
            .to_user(&self.origin)
            .await
            .context("failed to get discord user")?;

        CACHE
            .write()
            .await
            .insert((None, user_id), user.name.clone());

        match self.guild_id {
            Some(gid) => {
                let nick = gid
                    .member(&self.origin, user_id)
                    .await
                    .map(|x| x.nick)
                    .context("failed to get username from discord")?;

                if let Some(ref nick) = nick {
                    CACHE
                        .write()
                        .await
                        .insert((Some(gid), user_id), nick.clone());
                }

                Ok(nick.unwrap_or(user.name))
            }

            None => Ok(user.name),
        }
    }
}
