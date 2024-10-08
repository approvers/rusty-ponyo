use {
    crate::bot::{Attachment, BotService, Context, Message, SendMessage, User},
    anyhow::{Context as _, Result},
    async_trait::async_trait,
    serenity::{
        builder::{CreateAttachment, CreateMessage},
        model::{
            channel::{Attachment as SerenityAttachment, Message as SerenityMessage},
            gateway::Ready,
            id::{ChannelId as SerenityChannelId, UserId as SerenityUserId},
            voice::VoiceState,
        },
        prelude::{Client, Context as SerenityContext, EventHandler, GatewayIntents},
    },
    std::{
        collections::{HashMap, HashSet},
        future::Future,
        sync::Arc,
        time::Duration,
    },
    tokio::{
        sync::{Mutex, RwLock},
        time::interval,
    },
};

fn submit_signal_handler(client: &Client, waiter: impl Future + Send + 'static) {
    let shard_manager = client.shard_manager.clone();

    tokio::spawn(async move {
        waiter.await;
        shard_manager.shutdown_all().await;
    });
}

pub(crate) struct DiscordClient {
    services: Vec<Box<dyn BotService>>,
}

impl DiscordClient {
    pub fn new() -> Self {
        Self { services: vec![] }
    }

    pub fn add_service<S>(&mut self, service: S) -> &mut Self
    where
        S: BotService + Send + 'static,
    {
        self.services.push(Box::new(service));
        self
    }

    pub async fn run(self, token: &str) -> Result<()> {
        let event_handler = EvHandler::new(self.services);

        let mut client = Client::builder(token, GatewayIntents::all())
            .event_handler(event_handler)
            .await
            .context("Failed to create Discord client")?;

        submit_signal_handler(&client, async {
            tokio::signal::ctrl_c()
                .await
                .expect("could not register ctrl+c handler");
        });

        #[cfg(unix)]
        submit_signal_handler(&client, async {
            use tokio::signal::unix::{signal, SignalKind};
            signal(SignalKind::terminate())
                .expect("could not register SIGTERM handler")
                .recv()
                .await;
        });

        client
            .start()
            .await
            .context("Failed to start Discord client")
    }
}

// TODO: should be configurable
const APPROVERS_GUILD_ID: u64 = 683939861539192860;
const APPROVERS_DEFAULT_CHANNEL_ID: u64 = 690909527461199922;

struct EvHandlerInner {
    services: Vec<Box<dyn BotService>>,
    vc_joined_users: Mutex<HashSet<SerenityUserId>>,
    nickname_cache: RwLock<NicknameCache>,
    is_bot_cache: RwLock<IsBotCache>,
}

struct EvHandler {
    inner: Arc<EvHandlerInner>,
}

impl EvHandler {
    fn new(services: Vec<Box<dyn BotService>>) -> Self {
        Self {
            inner: Arc::new(EvHandlerInner {
                services,
                vc_joined_users: Mutex::new(HashSet::new()),
                nickname_cache: RwLock::new(NicknameCache(HashMap::new())),
                is_bot_cache: RwLock::new(IsBotCache(HashMap::new())),
            }),
        }
    }

    async fn do_for_each_service<'a, F, Fu>(
        ctx: &'a SerenityContext,
        inner: &'a EvHandlerInner,
        op: &'static str,
        f: F,
    ) where
        Fu: Future<Output = Result<()>> + Send + 'a,
        F: Fn(&'a dyn BotService) -> Fu,
    {
        for service in &inner.services {
            let result = f(service.as_ref()).await;

            if let Err(e) = result {
                tracing::error!(
                    "Service({})::{} returned error: {:?}",
                    service.name(),
                    op,
                    e
                );

                SerenityChannelId::new(APPROVERS_DEFAULT_CHANNEL_ID)
                    .say(
                        &ctx,
                        &format!("Unexpected error reported from \"{}\". Read log <@!391857452360007680>", service.name()),
                    )
                    .await
                    .ok();
            }
        }
    }

    async fn initial_validate_vc_cache(inner: Arc<EvHandlerInner>, ctx: SerenityContext) {
        let mut interval = interval(Duration::from_secs(1));

        loop {
            interval.tick().await;

            let guild = match ctx.cache.guild(APPROVERS_GUILD_ID) {
                Some(g) => g.clone(),
                None => continue,
            };

            let joined_users = guild
                .voice_states
                .keys()
                .map(|user_id| user_id.get())
                .collect::<Vec<_>>();

            for user_id in &joined_users {
                tracing::info!("joined users on startup: {}", user_id);

                inner
                    .vc_joined_users
                    .lock()
                    .await
                    .insert(SerenityUserId::new(*user_id));
            }

            let converted_ctx = DiscordContext::from_serenity(
                &ctx,
                APPROVERS_DEFAULT_CHANNEL_ID,
                &inner.nickname_cache,
                &inner.is_bot_cache,
            );

            Self::do_for_each_service(&ctx, &inner, "on_vc_data_available", |s| {
                s.on_vc_data_available(&converted_ctx, &joined_users)
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

            let guild = match ctx.cache.guild(APPROVERS_GUILD_ID) {
                Some(g) => g.clone(),
                None => {
                    tracing::warn!("missing guild in validate_vc_cache_loop. This is not good sign because mismatch of inner.vc_state can occur.");
                    continue;
                }
            };

            let converted_ctx = DiscordContext::from_serenity(
                &ctx,
                APPROVERS_DEFAULT_CHANNEL_ID,
                &inner.nickname_cache,
                &inner.is_bot_cache,
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
                tracing::info!("user({}) has actually joined to vc", uid.get());

                Self::do_for_each_service(&ctx, &inner, "on_vc_join", |s| {
                    s.on_vc_join(&converted_ctx, uid.get())
                })
                .await;
            }

            for uid in missing_in_serenity_state {
                tracing::info!("user({}) has actually left from vc", uid.get());

                self_state.remove(&uid);

                Self::do_for_each_service(&ctx, &inner, "on_vc_leave", |s| {
                    s.on_vc_leave(&converted_ctx, uid.get())
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
        tokio::spawn(Self::initial_validate_vc_cache(inner, ctx));
    }

    async fn voice_state_update(
        &self,
        ctx: SerenityContext,
        _: Option<VoiceState>,
        state: VoiceState,
    ) {
        let gid = state.guild_id;
        let is_approvers_event = gid.map(|x| x == APPROVERS_GUILD_ID).unwrap_or(false);
        if !is_approvers_event {
            return;
        }

        let user_id = state.user_id;
        let currently_joined = state.channel_id.is_some();

        let mut self_state = self.inner.vc_joined_users.lock().await;

        let self_state_currently_joined = self_state.iter().any(|x| *x == user_id);

        let converted_ctx = DiscordContext::from_serenity(
            &ctx,
            APPROVERS_DEFAULT_CHANNEL_ID,
            &self.inner.nickname_cache,
            &self.inner.is_bot_cache,
        );

        match (currently_joined, self_state_currently_joined) {
            // joined
            (true, false) => {
                tracing::debug!("User({}) has joined to vc", user_id.get());

                self_state.insert(user_id);

                Self::do_for_each_service(&ctx, &self.inner, "on_vc_join", |s| {
                    s.on_vc_join(&converted_ctx, user_id.get())
                })
                .await;
            }

            // left
            (false, true) => {
                tracing::debug!("User({}) has left from vc", user_id.get());

                self_state.remove(&user_id);

                Self::do_for_each_service(&ctx, &self.inner, "on_vc_leave", |s| {
                    s.on_vc_leave(&converted_ctx, user_id.get())
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

        self.inner
            .nickname_cache
            .write()
            .await
            .0
            .insert(message.author.id, message.author.name.clone());

        let converted_message = DiscordMessage {
            content: message.content.clone(),
            attachments: converted_attachments.iter().map(|x| x as _).collect(),
            author: DiscordAuthor {
                id: message.author.id.get(),
                name: message.author.name,
                ctx: &ctx,
            },
        };

        let converted_context = DiscordContext::from_serenity(
            &ctx,
            message.channel_id,
            &self.inner.nickname_cache,
            &self.inner.is_bot_cache,
        );

        Self::do_for_each_service(&ctx, &self.inner, "on_message", |s| {
            s.on_message(&converted_message, &converted_context)
        })
        .await;
    }
}

struct NicknameCache(HashMap<SerenityUserId, String>);

struct IsBotCache(HashMap<SerenityUserId, bool>);

struct DiscordMessage<'a> {
    content: String,
    attachments: Vec<&'a dyn Attachment>,
    author: DiscordAuthor<'a>,
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

struct DiscordAuthor<'a> {
    id: u64,
    #[allow(unused)]
    name: String,
    ctx: &'a SerenityContext,
}

#[async_trait]
impl<'a> User for DiscordAuthor<'a> {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    async fn dm(&self, msg: SendMessage<'_>) -> Result<()> {
        let files = msg
            .attachments
            .iter()
            .map(|x| CreateAttachment::bytes(x.data, x.name))
            .collect::<Vec<_>>();

        let msg = CreateMessage::new().content(msg.content);

        SerenityUserId::new(self.id)
            .create_dm_channel(self.ctx)
            .await
            .context("failed to create DM channel")?
            .send_files(self.ctx, files, msg)
            .await
            .context("failed to send DM")?;

        Ok(())
    }
}

struct DiscordAttachment<'a>(&'a SerenityAttachment);

#[async_trait]
impl Attachment for DiscordAttachment<'_> {
    fn name(&self) -> &str {
        &self.0.filename
    }

    fn size(&self) -> usize {
        self.0.size as usize
    }

    async fn download(&self) -> Result<Vec<u8>> {
        self.0
            .download()
            .await
            .context("failed to download attachment from discord")
    }
}

struct DiscordContext<'a> {
    origin: &'a SerenityContext,
    channel_id: SerenityChannelId,
    nickname_cache: &'a RwLock<NicknameCache>,
    is_bot_cache: &'a RwLock<IsBotCache>,
}

impl<'a> DiscordContext<'a> {
    fn from_serenity(
        origin: &'a SerenityContext,
        channel_id: impl Into<SerenityChannelId>,
        nickname_cache: &'a RwLock<NicknameCache>,
        is_bot_cache: &'a RwLock<IsBotCache>,
    ) -> Self {
        Self {
            origin,
            channel_id: channel_id.into(),
            nickname_cache,
            is_bot_cache,
        }
    }
}

#[async_trait]
impl Context for DiscordContext<'_> {
    async fn send_message(&self, msg: SendMessage<'_>) -> Result<()> {
        let files = msg
            .attachments
            .iter()
            .map(|x| CreateAttachment::bytes(x.data, x.name))
            .collect::<Vec<_>>();

        let msg = CreateMessage::new().content(msg.content);

        self.channel_id
            .send_files(&self.origin.http, files, msg)
            .await
            .context("failed to send message to discord")?;

        Ok(())
    }

    async fn get_user_name(&self, user_id: u64) -> Result<String> {
        let user_id = SerenityUserId::new(user_id);

        if let Some(nick) = self.nickname_cache.read().await.0.get(&user_id) {
            return Ok(nick.to_string());
        }

        let user = user_id
            .to_user(self.origin)
            .await
            .context("failed to get username from discord")?;

        self.nickname_cache
            .write()
            .await
            .0
            .insert(user_id, user.name.clone());

        self.is_bot_cache.write().await.0.insert(user_id, user.bot);

        return Ok(user.name);
    }

    async fn is_bot(&self, user_id: u64) -> Result<bool> {
        let user_id = SerenityUserId::new(user_id);

        if let Some(&bot) = self.is_bot_cache.read().await.0.get(&user_id) {
            return Ok(bot);
        }

        let user = user_id
            .to_user(self.origin)
            .await
            .context("failed to get username from discord")?;

        self.nickname_cache
            .write()
            .await
            .0
            .insert(user_id, user.name.clone());

        self.is_bot_cache.write().await.0.insert(user_id, user.bot);

        return Ok(user.bot);
    }
}
