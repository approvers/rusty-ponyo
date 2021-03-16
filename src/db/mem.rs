use {
    crate::bot::{
        alias::{model::MessageAlias, MessageAliasDatabase},
        genkai_point::{
            model::{Session, UserStat},
            GenkaiPointDatabase,
        },
    },
    anyhow::{anyhow, Context as _, Result},
    async_trait::async_trait,
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
        if let Some(e) = self.aliases.iter().find(|x| x.key == key) {
            return Ok(Some(e.clone()));
        }

        Ok(None)
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
}

#[async_trait]
impl GenkaiPointDatabase for MemoryDB {
    async fn create_new_session(
        &mut self,
        user_id: u64,
        joined_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool> {
        if self.unclosed_session_exists(user_id).await.unwrap() {
            return Ok(false);
        }

        self.sessions.push(Session {
            user_id,
            joined_at,
            left_at: None,
        });

        self.dump().await?;

        Ok(true)
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
                    });
                }
            }
        }

        Ok(result)
    }
}
