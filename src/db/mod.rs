pub mod mem;
pub mod mongodb;

use {anyhow::Result, async_trait::async_trait};

#[async_trait]
pub(crate) trait MessageAliasDatabase: Send + Sync + 'static {
    async fn save(&mut self, key: &str, msg: &str) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Option<String>>;
    async fn delete(&mut self, key: &str) -> Result<bool>;
    async fn len(&self) -> Result<u32>;
}
