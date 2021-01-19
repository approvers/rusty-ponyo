use {
    super::MessageAliasDatabase,
    crate::model::{MessageAlias, MessageAliasRef},
    anyhow::{Context as _, Result},
    async_trait::async_trait,
    mongodb::{
        bson::{self, doc},
        options::ClientOptions,
        Client, Database,
    },
    tokio::stream::StreamExt,
};

pub(crate) struct MongoDB {
    inner: Database,
}

impl MongoDB {
    pub(crate) async fn new(uri: &str) -> Result<Self> {
        let opt = ClientOptions::parse(uri)
            .await
            .context("failed to parse mongodb uri")?;

        let db = Client::with_options(opt)
            .context("failed to create mongodb client")?
            .database("RustyPonyo");

        Ok(Self { inner: db })
    }
}

#[async_trait]
impl MessageAliasDatabase for MongoDB {
    async fn save(&mut self, key: &str, message: &str) -> Result<()> {
        let alias = MessageAliasRef { key, message };
        let doc = bson::to_document(&alias).context("failed to serialize alias")?;

        self.inner
            .collection("MessageAlias")
            .insert_one(doc, None)
            .await
            .context("failed to insert new alias")?;

        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<String>> {
        self.inner
            .collection("MessageAlias")
            .find_one(doc! { "key": key }, None)
            .await
            .context("failed to fetch alias")?
            .map(bson::from_document::<MessageAlias>)
            .transpose()
            .context("failed to deserialize alias")
            .map(|x| x.map(|x| x.message))
    }

    async fn len(&self) -> Result<u32> {
        self.inner
            .collection("MessageAlias")
            .aggregate(vec![doc! { "$count": "key" }], None)
            .await
            .context("failed to aggregate")?
            .next()
            .await
            .context("aggregation returned nothing")?
            .context("failed to fetch aggregation result")?
            .get("key")
            .context("aggregation result didn't have \"key\" property")?
            .as_i32()
            .context("aggregation result's key property was not i32")
            .map(|x| x as u32)
    }
}
