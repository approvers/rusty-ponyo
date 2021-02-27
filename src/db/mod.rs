use crate::{model::MessageAlias, ThreadSafe};

#[cfg(feature = "memory_db")]
pub mod mem;

#[cfg(feature = "mongo_db")]
pub mod mongodb;

use {anyhow::Result, async_trait::async_trait};

#[async_trait]
pub(crate) trait MessageAliasDatabase: ThreadSafe {
    async fn save(&mut self, alias: MessageAlias) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Option<MessageAlias>>;
    async fn delete(&mut self, key: &str) -> Result<bool>;
    async fn len(&self) -> Result<u32>;
}
