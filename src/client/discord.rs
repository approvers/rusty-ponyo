use {
    super::ServiceVisitor,
    crate::{
        bot::{Attachment, BotService, Context, Message, Runtime, SendMessage, User},
        client::{ListCons, ListNil, ServiceList},
    },
    anyhow::{Context as _, Result},
    rusty_ponyo::{APPROVERS_DEFAULT_CHANNEL_ID, APPROVERS_GUILD_ID},
    serenity::{
        async_trait,
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

pub struct DiscordClient<L: ServiceList<DiscordRuntime>> {
    services: L,
}
impl DiscordClient<ListNil> {
    pub fn new() -> Self {
        Self { services: ListNil }
    }
}

impl<L: ServiceList<DiscordRuntime> + 'static> DiscordClient<L> {
    pub fn add_service<S>(self, service: S) -> DiscordClient<ListCons<DiscordRuntime, S, L>>
    where
        S: BotService<DiscordRuntime> + Send + 'static,
    {
        DiscordClient {
            services: self.services.append(service),
        }
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
            use tokio::signal::unix::{SignalKind, signal};
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

struct EvHandlerInner<L: ServiceList<DiscordRuntime>> {
    services: L,
    vc_joined_users: Mutex<HashSet<SerenityUserId>>,
    nickname_cache: Arc<RwLock<NicknameCache>>,
    is_bot_cache: Arc<RwLock<IsBotCache>>,
}

struct EvHandler<L: ServiceList<DiscordRuntime>> {
    inner: Arc<EvHandlerInner<L>>,
}

// because current Rust cannot do this:
//   impl Fn(&impl BotService) -> impl Future;
mod visitors {
    use super::*;
    pub trait ForEachService: Send + Sync {
        const OP: &'static str;
        fn accept(
            &self,
            s: &impl BotService<DiscordRuntime>,
        ) -> impl Future<Output = Result<()>> + Send;
    }

    pub struct MessageVisitor<'a> {
        pub msg: &'a DiscordMessage,
        pub ctx: &'a DiscordContext,
    }
    impl ForEachService for MessageVisitor<'_> {
        const OP: &'static str = "on_message";
        async fn accept(&self, s: &impl BotService<DiscordRuntime>) -> Result<()> {
            s.on_message(self.msg, self.ctx).await
        }
    }

    pub struct VcDataAvailableVisitor<'a> {
        pub ctx: &'a DiscordContext,
        pub users: &'a [u64],
    }
    impl ForEachService for VcDataAvailableVisitor<'_> {
        const OP: &'static str = "on_vc_data_avaialble";
        async fn accept(&self, s: &impl BotService<DiscordRuntime>) -> Result<()> {
            s.on_vc_data_available(self.ctx, self.users).await
        }
    }

    pub struct VcJoinVisitor<'a> {
        pub ctx: &'a DiscordContext,
        pub uid: u64,
    }
    impl ForEachService for VcJoinVisitor<'_> {
        const OP: &'static str = "on_vc_join";
        async fn accept(&self, s: &impl BotService<DiscordRuntime>) -> Result<()> {
            s.on_vc_join(self.ctx, self.uid).await
        }
    }

    pub struct VcLeaveVisitor<'a> {
        pub ctx: &'a DiscordContext,
        pub uid: u64,
    }
    impl ForEachService for VcLeaveVisitor<'_> {
        const OP: &'static str = "on_vc_leave";
        async fn accept(&self, s: &impl BotService<DiscordRuntime>) -> Result<()> {
            s.on_vc_leave(self.ctx, self.uid).await
        }
    }
}

impl<L: ServiceList<DiscordRuntime> + 'static> EvHandler<L> {
    fn new(services: L) -> Self {
        Self {
            inner: Arc::new(EvHandlerInner {
                services,
                vc_joined_users: Mutex::new(HashSet::new()),
                nickname_cache: Arc::new(RwLock::new(NicknameCache(HashMap::new()))),
                is_bot_cache: Arc::new(RwLock::new(IsBotCache(HashMap::new()))),
            }),
        }
    }

    async fn do_for_each_service<'a>(
        ctx: &'a SerenityContext,
        inner: &'a EvHandlerInner<L>,
        f: impl visitors::ForEachService,
    ) {
        inner.services.visit(&Visitor { ctx, f }).await;
        struct Visitor<'a, F: visitors::ForEachService> {
            ctx: &'a SerenityContext,
            f: F,
        }
        impl<F: visitors::ForEachService> ServiceVisitor<DiscordRuntime> for Visitor<'_, F> {
            async fn visit(&self, service: &impl BotService<DiscordRuntime>) {
                let result = self.f.accept(service).await;

                if let Err(e) = result {
                    tracing::error!(
                        "Service({})::{} returned error: {e:?}",
                        F::OP,
                        service.name()
                    );

                    SerenityChannelId::new(APPROVERS_DEFAULT_CHANNEL_ID)
                        .say(
                            &self.ctx,
                            &format!("Unexpected error reported from \"{}\". Read log <@!391857452360007680>", service.name()),
                        )
                        .await
                        .ok();
                }
            }
        }
    }

    async fn initial_validate_vc_cache(inner: Arc<EvHandlerInner<L>>, ctx: SerenityContext) {
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

            Self::do_for_each_service(
                &ctx,
                &inner,
                visitors::VcDataAvailableVisitor {
                    ctx: &converted_ctx,
                    users: &joined_users,
                },
            )
            .await;

            tracing::info!("vc status checking on startup complete");
            break;
        }

        tokio::spawn(Self::validate_vc_cache_loop(inner, ctx));
    }

    async fn validate_vc_cache_loop(inner: Arc<EvHandlerInner<L>>, ctx: SerenityContext) {
        let mut interval = interval(Duration::from_secs(30));

        loop {
            interval.tick().await;

            let guild = match ctx.cache.guild(APPROVERS_GUILD_ID) {
                Some(g) => g.clone(),
                None => {
                    tracing::warn!(
                        "missing guild in validate_vc_cache_loop. This is not good sign because mismatch of inner.vc_state can occur."
                    );
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

                Self::do_for_each_service(
                    &ctx,
                    &inner,
                    visitors::VcJoinVisitor {
                        ctx: &converted_ctx,
                        uid: uid.get(),
                    },
                )
                .await;
            }

            for uid in missing_in_serenity_state {
                tracing::info!("user({}) has actually left from vc", uid.get());

                self_state.remove(&uid);

                Self::do_for_each_service(
                    &ctx,
                    &inner,
                    visitors::VcLeaveVisitor {
                        ctx: &converted_ctx,
                        uid: uid.get(),
                    },
                )
                .await;
            }
        }
    }
}

#[async_trait]
impl<L: ServiceList<DiscordRuntime> + Send + Sync + 'static> EventHandler for EvHandler<L> {
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

                Self::do_for_each_service(
                    &ctx,
                    &self.inner,
                    visitors::VcJoinVisitor {
                        ctx: &converted_ctx,
                        uid: user_id.get(),
                    },
                )
                .await;
            }

            // left
            (false, true) => {
                tracing::debug!("User({}) has left from vc", user_id.get());

                self_state.remove(&user_id);

                Self::do_for_each_service(
                    &ctx,
                    &self.inner,
                    visitors::VcLeaveVisitor {
                        ctx: &converted_ctx,
                        uid: user_id.get(),
                    },
                )
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
            .clone()
            .into_iter()
            .map(DiscordAttachment)
            .collect::<Vec<_>>();

        self.inner
            .nickname_cache
            .write()
            .await
            .0
            .insert(message.author.id, message.author.name.clone());

        let converted_context = DiscordContext::from_serenity(
            &ctx,
            message.channel_id,
            &self.inner.nickname_cache,
            &self.inner.is_bot_cache,
        );

        let converted_message = DiscordMessage {
            ctx: ctx.clone(),
            attachments: converted_attachments,
            author: DiscordAuthor {
                id: message.author.id.get(),
                name: message.author.name.clone(),
                ctx: ctx.clone(),
            },
            message,
        };

        Self::do_for_each_service(
            &ctx,
            &self.inner,
            visitors::MessageVisitor {
                msg: &converted_message,
                ctx: &converted_context,
            },
        )
        .await;
    }
}

pub struct DiscordRuntime;
impl Runtime for DiscordRuntime {
    type Message = DiscordMessage;
    type Context = DiscordContext;
}

struct NicknameCache(HashMap<SerenityUserId, String>);

struct IsBotCache(HashMap<SerenityUserId, bool>);

pub struct DiscordMessage {
    ctx: SerenityContext,
    message: SerenityMessage,
    attachments: Vec<DiscordAttachment>,
    author: DiscordAuthor,
}

impl Message for DiscordMessage {
    type Attachment = DiscordAttachment;
    type User = DiscordAuthor;

    async fn reply(&self, text: &str) -> Result<()> {
        self.message
            .reply_ping(&self.ctx.http, text)
            .await
            .context("failed to reply with discord feature")?;
        Ok(())
    }

    fn content(&self) -> &str {
        &self.message.content
    }

    fn attachments(&self) -> &[DiscordAttachment] {
        &self.attachments
    }

    fn author(&self) -> &DiscordAuthor {
        &self.author
    }
}

pub struct DiscordAuthor {
    id: u64,
    #[allow(unused)]
    name: String,
    ctx: SerenityContext,
}

impl User for DiscordAuthor {
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
            .create_dm_channel(&self.ctx)
            .await
            .context("failed to create DM channel")?
            .send_files(&self.ctx, files, msg)
            .await
            .context("failed to send DM")?;

        Ok(())
    }
}

pub struct DiscordAttachment(SerenityAttachment);

impl Attachment for DiscordAttachment {
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

pub struct DiscordContext {
    origin: SerenityContext,
    channel_id: SerenityChannelId,
    nickname_cache: Arc<RwLock<NicknameCache>>,
    is_bot_cache: Arc<RwLock<IsBotCache>>,
}

impl DiscordContext {
    fn from_serenity(
        origin: &SerenityContext,
        channel_id: impl Into<SerenityChannelId>,
        nickname_cache: &Arc<RwLock<NicknameCache>>,
        is_bot_cache: &Arc<RwLock<IsBotCache>>,
    ) -> Self {
        Self {
            origin: origin.clone(),
            channel_id: channel_id.into(),
            nickname_cache: Arc::clone(nickname_cache),
            is_bot_cache: Arc::clone(is_bot_cache),
        }
    }
}

impl Context for DiscordContext {
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
            .to_user(&self.origin)
            .await
            .context("failed to get username from discord")?;

        self.nickname_cache
            .write()
            .await
            .0
            .insert(user_id, user.name.clone());

        self.is_bot_cache.write().await.0.insert(user_id, user.bot);

        Ok(user.name)
    }

    async fn is_bot(&self, user_id: u64) -> Result<bool> {
        let user_id = SerenityUserId::new(user_id);

        if let Some(&bot) = self.is_bot_cache.read().await.0.get(&user_id) {
            return Ok(bot);
        }

        let user = user_id
            .to_user(&self.origin)
            .await
            .context("failed to get username from discord")?;

        self.nickname_cache
            .write()
            .await
            .0
            .insert(user_id, user.name.clone());

        self.is_bot_cache.write().await.0.insert(user_id, user.bot);

        Ok(user.bot)
    }
}
