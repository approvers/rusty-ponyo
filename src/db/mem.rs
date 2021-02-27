use {
    crate::{db::MessageAliasDatabase, model::MessageAlias},
    anyhow::Result,
    async_trait::async_trait,
};

pub(crate) struct MemoryDB {
    inner: Vec<MessageAlias>,
}

impl MemoryDB {
    pub(crate) fn new() -> Self {
        Self { inner: vec![] }
    }
}

#[async_trait]
impl MessageAliasDatabase for MemoryDB {
    async fn save(&mut self, alias: MessageAlias) -> Result<()> {
        self.inner.push(alias);
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<MessageAlias>> {
        if let Some(e) = self.inner.iter().find(|x| x.key == key) {
            return Ok(Some(e.clone()));
        }

        Ok(None)
    }

    async fn delete(&mut self, key: &str) -> Result<bool> {
        let index = self.inner.iter().position(|x| x.key == key);

        if let Some(index) = index {
            self.inner.remove(index);
        }

        return Ok(index.is_some());
    }

    async fn len(&self) -> Result<u32> {
        Ok(self.inner.len() as _)
    }
}
