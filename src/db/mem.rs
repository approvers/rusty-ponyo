use {crate::db::MessageAliasDatabase, anyhow::Result, async_trait::async_trait};

struct AliasEntry {
    key: String,
    value: String,
}

pub(crate) struct MemoryDB {
    inner: Vec<AliasEntry>,
}

impl MemoryDB {
    pub(crate) fn new() -> Self {
        Self { inner: vec![] }
    }
}

#[async_trait]
impl MessageAliasDatabase for MemoryDB {
    async fn save(&mut self, key: &str, msg: &str) -> Result<()> {
        self.inner.push(AliasEntry {
            key: key.into(),
            value: msg.into(),
        });
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<String>> {
        if let Some(e) = self.inner.iter().find(|x| x.key == key) {
            return Ok(Some(e.value.clone()));
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
