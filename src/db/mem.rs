use {
    crate::bot::{
        alias::{model::MessageAlias, MessageAliasDatabase},
        genkai_point::{
            model::{Session, UserStat, GENKAI_POINT_MAX},
            CreateNewSessionResult, GenkaiPointDatabase,
        },
    },
    anyhow::{anyhow, Context as _, Result},
    async_trait::async_trait,
    chrono::{DateTime, Duration, Utc},
    serde::Serialize,
};

#[derive(Serialize)]
pub(crate) struct MemoryDB {
    aliases: Vec<MessageAlias>,
    sessions: Vec<Session>,
}

impl MemoryDB {
    pub(crate) fn new() -> Self {
        Self {
            aliases: vec![],
            sessions: vec![],
        }
    }

    pub(crate) async fn dump(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(self).context("failed to serialize")?;

        tokio::fs::write("mem_db_dump.json", json)
            .await
            .context("failed to dump to mem_db_dump.json")?;

        Ok(())
    }
}

#[async_trait]
impl MessageAliasDatabase for MemoryDB {
    async fn save(&mut self, alias: MessageAlias) -> Result<()> {
        self.aliases.push(alias);
        self.dump().await?;

        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<MessageAlias>> {
        Ok(self.aliases.iter().find(|x| x.key == key).cloned())
    }

    async fn get_and_increment_usage_count(&mut self, key: &str) -> Result<Option<MessageAlias>> {
        let e = self.get(key).await;

        if let Ok(Some(_)) = e {
            self.aliases
                .iter_mut()
                .find(|x| x.key == key)
                .unwrap()
                .usage_count += 1;
        }

        e
    }

    async fn delete(&mut self, key: &str) -> Result<bool> {
        let index = self.aliases.iter().position(|x| x.key == key);

        if let Some(index) = index {
            self.aliases.remove(index);
        }

        self.dump().await?;

        Ok(index.is_some())
    }

    async fn len(&self) -> Result<u32> {
        Ok(self.aliases.len() as _)
    }

    async fn usage_count_top_n(&self, n: usize) -> Result<Vec<MessageAlias>> {
        let mut p = self.aliases.clone();
        p.sort_by_key(|x| x.usage_count);
        p.truncate(n);

        Ok(p)
    }
}

#[async_trait]
impl GenkaiPointDatabase for MemoryDB {
    async fn create_new_session(
        &mut self,
        user_id: u64,
        joined_at: DateTime<Utc>,
    ) -> Result<CreateNewSessionResult> {
        if self.unclosed_session_exists(user_id).await.unwrap() {
            return Ok(CreateNewSessionResult::UnclosedSessionExists);
        }

        self.sessions.sort_unstable_by_key(|x| x.joined_at);

        if let Some(session) = self
            .sessions
            .iter_mut()
            .rev()
            .find(|x| x.user_id == user_id)
        {
            if let Some(left_at) = session.left_at {
                if (Utc::now() - left_at) < Duration::minutes(5) {
                    session.left_at = None;
                    self.dump().await?;
                    return Ok(CreateNewSessionResult::SessionResumed);
                }
            }
        }

        self.sessions.push(Session {
            user_id,
            joined_at,
            left_at: None,
        });

        self.dump().await?;

        Ok(CreateNewSessionResult::CreatedNewSession)
    }

    async fn unclosed_session_exists(&self, user_id: u64) -> Result<bool> {
        Ok(self
            .sessions
            .iter()
            .filter(|x| x.user_id == user_id)
            .any(|x| x.left_at.is_none()))
    }

    async fn close_session(
        &mut self,
        user_id: u64,
        left_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        self.sessions
            .iter_mut()
            .filter(|x| x.user_id == user_id)
            .find(|x| x.left_at.is_none())
            .ok_or_else(|| anyhow!("there is no unclosed session"))?
            .left_at = Some(left_at);

        self.dump().await?;

        Ok(())
    }

    async fn get_all_users_who_has_unclosed_session(&self) -> Result<Vec<u64>> {
        let mut list = self
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
            .sessions
            .iter()
            .filter(|x| x.user_id == user_id)
            .cloned()
            .collect())
    }

    async fn get_all_users_stats(&self) -> Result<Vec<UserStat>> {
        let mut result: Vec<UserStat> = vec![];

        for session in &self.sessions {
            match result.iter_mut().find(|x| x.user_id == session.user_id) {
                Some(stat) => {
                    stat.genkai_point += session.calc_point();
                    // += is not implemented on chrono::Duration
                    stat.total_vc_duration = stat.total_vc_duration + session.duration();
                }

                None => {
                    result.push(UserStat {
                        user_id: session.user_id,
                        genkai_point: session.calc_point(),
                        total_vc_duration: session.duration(),
                        efficiency: 0.0,
                    });
                }
            }
        }

        for stat in &mut result {
            stat.efficiency = (stat.genkai_point as f64 / GENKAI_POINT_MAX as f64)
                / (stat.total_vc_duration.num_minutes() as f64 / 60.0);
        }

        Ok(result)
    }
}
