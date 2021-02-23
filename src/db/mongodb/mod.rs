mod model;

use {
    crate::{
        db::{mongodb::model::MongoMessageAlias, MessageAliasDatabase},
        model::MessageAlias,
    },
    anyhow::{Context as _, Result},
    async_trait::async_trait,
    mongodb::{
        bson::{self, doc},
        options::ClientOptions,
        Client, Database,
    },
    tokio_stream::StreamExt,
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

const MESSAGE_ALIAS_COLLECTION_NAME: &str = "MessageAlias";

#[async_trait]
impl MessageAliasDatabase for MongoDB {
    async fn save(&mut self, alias: MessageAlias) -> Result<()> {
        let alias: MongoMessageAlias = alias.into();
        let doc = bson::to_document(&alias).context("failed to serialize alias")?;

        self.inner
            .collection(MESSAGE_ALIAS_COLLECTION_NAME)
            .insert_one(doc, None)
            .await
            .context("failed to insert new alias")?;

        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<MessageAlias>> {
        self.inner
            .collection(MESSAGE_ALIAS_COLLECTION_NAME)
            .find_one(doc! { "key": key }, None)
            .await
            .context("failed to fetch alias")?
            .map(bson::from_document::<MongoMessageAlias>)
            .transpose()
            .context("failed to deserialize alias")
            .map(|x| x.map(|x| x.into()))
    }

    async fn len(&self) -> Result<u32> {
        self.inner
            .collection(MESSAGE_ALIAS_COLLECTION_NAME)
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

    async fn delete(&mut self, key: &str) -> Result<bool> {
        self.inner
            .collection(MESSAGE_ALIAS_COLLECTION_NAME)
            .delete_one(doc! { "key": key }, None)
            .await
            .context("failed to delete alias")
            .map(|x| x.deleted_count == 1)
    }
}
