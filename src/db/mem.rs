use {
    crate::bot::{
        alias::{model::MessageAlias, MessageAliasDatabase},
        auth::GenkaiAuthDatabase,
        genkai_point::{model::Session, CreateNewSessionResult, GenkaiPointDatabase},
        meigen::{
            self,
            model::{Meigen, MeigenId},
            MeigenDatabase, SortDirection, SortKey,
        },
        IsUpdated,
    },
    anyhow::{anyhow, Context as _, Result},
    chrono::{DateTime, Duration, Utc},
    rand::{seq::SliceRandom, thread_rng},
    serde::Serialize,
    std::{collections::HashMap, ops::DerefMut, sync::Arc},
    tokio::sync::Mutex,
};

#[derive(Serialize)]
struct MemoryDBInner {
    aliases: Vec<MessageAlias>,
    sessions: Vec<Session>,
    auth_entries: HashMap<u64, AuthEntry>,
    meigens: Vec<Meigen>,
}

pub struct MemoryDB(Arc<Mutex<MemoryDBInner>>);

impl Clone for MemoryDB {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl MemoryDB {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(MemoryDBInner {
            aliases: vec![],
            sessions: vec![],
            auth_entries: HashMap::new(),
            meigens: vec![],
        })))
    }

    async fn inner(&self) -> impl DerefMut<Target = MemoryDBInner> + DerefMut + '_ {
        self.0.lock().await
    }
}

impl MemoryDBInner {
    pub async fn dump(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(self).context("failed to serialize")?;

        tokio::fs::write("mem_db_dump.json", json)
            .await
            .context("failed to dump to mem_db_dump.json")?;

        Ok(())
    }
}

impl MessageAliasDatabase for MemoryDB {
    async fn save(&self, alias: MessageAlias) -> Result<()> {
        let mut me = self.inner().await;
        me.aliases.push(alias);
        me.dump().await?;

        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<MessageAlias>> {
        Ok(self
            .inner()
            .await
            .aliases
            .iter()
            .find(|x| x.key == key)
            .cloned())
    }

    async fn get_and_increment_usage_count(&self, key: &str) -> Result<Option<MessageAlias>> {
        let e = self.get(key).await;

        if let Ok(Some(_)) = e {
            self.inner()
                .await
                .aliases
                .iter_mut()
                .find(|x| x.key == key)
                .unwrap()
                .usage_count += 1;
        }

        e
    }

    async fn delete(&self, key: &str) -> Result<IsUpdated> {
        let mut me = self.inner().await;
        let index = me.aliases.iter().position(|x| x.key == key);

        if let Some(index) = index {
            me.aliases.remove(index);
        }

        me.dump().await?;

        Ok(index.is_some())
    }

    async fn len(&self) -> Result<u32> {
        Ok(self.inner().await.aliases.len() as _)
    }

    async fn usage_count_top_n(&self, n: usize) -> Result<Vec<MessageAlias>> {
        let mut p = self.inner().await.aliases.clone();
        p.sort_by_key(|x| x.usage_count);
        p.truncate(n);

        Ok(p)
    }
}

impl GenkaiPointDatabase for MemoryDB {
    async fn create_new_session(
        &self,
        user_id: u64,
        joined_at: DateTime<Utc>,
    ) -> Result<CreateNewSessionResult> {
        if self.unclosed_session_exists(user_id).await.unwrap() {
            return Ok(CreateNewSessionResult::UnclosedSessionExists);
        }

        let mut me = self.inner().await;
        me.sessions.sort_unstable_by_key(|x| x.joined_at);

        if let Some(session) = me.sessions.iter_mut().rev().find(|x| x.user_id == user_id) {
            if let Some(left_at) = session.left_at {
                if (Utc::now() - left_at) < Duration::minutes(5) {
                    session.left_at = None;
                    me.dump().await?;
                    return Ok(CreateNewSessionResult::SessionResumed);
                }
            }
        }

        me.sessions.push(Session {
            user_id,
            joined_at,
            left_at: None,
        });

        me.dump().await?;

        Ok(CreateNewSessionResult::NewSessionCreated)
    }

    async fn unclosed_session_exists(&self, user_id: u64) -> Result<bool> {
        Ok(self
            .0
            .lock()
            .await
            .sessions
            .iter()
            .filter(|x| x.user_id == user_id)
            .any(|x| x.left_at.is_none()))
    }

    async fn close_session(
        &self,
        user_id: u64,
        left_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let mut me = self.inner().await;

        me.sessions
            .iter_mut()
            .filter(|x| x.user_id == user_id)
            .find(|x| x.left_at.is_none())
            .ok_or_else(|| anyhow!("there is no unclosed session"))?
            .left_at = Some(left_at);

        me.dump().await?;

        Ok(())
    }

    async fn get_all_users_who_has_unclosed_session(&self) -> Result<Vec<u64>> {
        let mut list = self
            .0
            .lock()
            .await
            .sessions
            .iter()
            .filter(|x| x.left_at.is_none())
            .map(|x| x.user_id)
            .collect::<Vec<_>>();

        list.dedup();

        Ok(list)
    }

    async fn get_users_all_sessions(&self, user_id: u64) -> Result<Vec<Session>> {
        Ok(self
            .0
            .lock()
            .await
            .sessions
            .iter()
            .filter(|x| x.user_id == user_id)
            .cloned()
            .collect())
    }

    async fn get_all_sessions(&self) -> Result<Vec<Session>> {
        Ok(self.inner().await.sessions.clone())
    }
}

#[derive(Serialize, Default)]
struct AuthEntry {
    pgp_pub_key: Option<String>,
    token: Option<String>,
}

impl GenkaiAuthDatabase for MemoryDB {
    async fn register_pgp_key(&self, user_id: u64, key: &str) -> Result<()> {
        self.inner()
            .await
            .auth_entries
            .entry(user_id)
            .or_default()
            .pgp_pub_key = Some(key.to_string());

        Ok(())
    }

    async fn get_pgp_key(&self, user_id: u64) -> Result<Option<String>> {
        Ok(self
            .inner()
            .await
            .auth_entries
            .get(&user_id)
            .and_then(|x| x.pgp_pub_key.clone()))
    }

    async fn register_token(&self, user_id: u64, token: &str) -> Result<()> {
        self.inner()
            .await
            .auth_entries
            .entry(user_id)
            .or_default()
            .token = Some(token.to_string());

        Ok(())
    }

    async fn revoke_token(&self, user_id: u64) -> Result<()> {
        self.inner()
            .await
            .auth_entries
            .entry(user_id)
            .or_default()
            .token = None;

        Ok(())
    }

    async fn get_token(&self, user_id: u64) -> Result<Option<String>> {
        Ok(self
            .inner()
            .await
            .auth_entries
            .get(&user_id)
            .and_then(|x| x.token.clone()))
    }
}

impl MeigenDatabase for MemoryDB {
    async fn save(
        &self,
        author: impl Into<String> + Send,
        content: impl Into<String> + Send,
    ) -> Result<Meigen> {
        let author = author.into();
        let content = content.into();

        let mut inner = self.inner().await;
        let current_id = inner.meigens.iter().map(|x| x.id.0).max().unwrap_or(0);
        let meigen = Meigen {
            id: MeigenId(current_id).succ(),
            author,
            content,
            loved_user_id: vec![],
        };

        inner.meigens.push(meigen.clone());

        Ok(meigen)
    }

    async fn load(&self, id: MeigenId) -> Result<Option<Meigen>> {
        Ok(self
            .inner()
            .await
            .meigens
            .iter()
            .find(|x| x.id == id)
            .cloned())
    }

    async fn delete(&self, id: MeigenId) -> Result<IsUpdated> {
        let mut inner = self.inner().await;
        let Some(index) = inner.meigens.iter().position(|x| x.id == id) else {
            return Ok(false);
        };

        inner.meigens.remove(index);

        Ok(true)
    }

    async fn search(&self, options: meigen::FindOptions<'_>) -> Result<Vec<Meigen>> {
        let inner = self.inner().await;
        let mut meigens = inner
            .meigens
            .iter()
            .filter(|x| {
                options.author.map_or(true, |a| x.author.contains(a))
                    && options.content.map_or(true, |c| x.content.contains(c))
            })
            .collect::<Vec<_>>();

        if options.random {
            meigens.shuffle(&mut thread_rng());
            meigens.truncate(options.limit as usize);
        }

        match options.sort {
            SortKey::Id => meigens.sort_by_key_with_dir(|meigen| meigen.id, options.dir),
            SortKey::Love => {
                meigens.sort_by_key_with_dir(|meigen| meigen.loved_user_id.len(), options.dir)
            }
            SortKey::Length => {
                meigens.sort_by_key_with_dir(|meigen| meigen.content.len(), options.dir)
            }
        }

        Ok(meigens
            .into_iter()
            .skip(if options.random {
                0
            } else {
                options.offset as usize
            })
            .take(options.limit as usize)
            .cloned()
            .collect())
    }

    async fn count(&self) -> Result<u32> {
        Ok(self.inner().await.meigens.len() as u32)
    }

    async fn append_loved_user(&self, id: MeigenId, loved_user_id: u64) -> Result<IsUpdated> {
        let mut inner = self.inner().await;
        let Some(meigen) = inner.meigens.iter_mut().find(|x| x.id == id) else {
            return Ok(false);
        };

        if meigen.loved_user_id.contains(&loved_user_id) {
            return Ok(false);
        }

        meigen.loved_user_id.push(loved_user_id);
        Ok(true)
    }

    async fn remove_loved_user(&self, id: MeigenId, loved_user_id: u64) -> Result<IsUpdated> {
        let mut inner = self.inner().await;
        let Some(meigen) = inner.meigens.iter_mut().find(|x| x.id == id) else {
            return Ok(false);
        };
        let Some(index) = meigen
            .loved_user_id
            .iter()
            .position(|&x| x == loved_user_id)
        else {
            return Ok(false);
        };

        meigen.loved_user_id.swap_remove(index);
        Ok(true)
    }
}

pub trait SortByKeyWithDirTraitExt<I> {
    fn sort_by_key_with_dir<FnK, FnKR>(&mut self, key: FnK, dir: SortDirection)
    where
        FnK: Fn(&I) -> FnKR,
        FnKR: Ord;
}

impl<I> SortByKeyWithDirTraitExt<I> for Vec<I> {
    fn sort_by_key_with_dir<FnK, FnKR>(&mut self, key: FnK, dir: SortDirection)
    where
        FnK: Fn(&I) -> FnKR,
        FnKR: Ord,
    {
        self.sort_by(|left, right| {
            let left_key = key(left);
            let right_key = key(right);

            match dir {
                SortDirection::Asc => left_key.cmp(&right_key),
                SortDirection::Desc => left_key.cmp(&right_key).reverse(),
            }
        })
    }
}
