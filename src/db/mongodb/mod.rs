mod model;

use {
    crate::{
        bot::{
            alias::{model::MessageAlias, MessageAliasDatabase},
            genkai_point::{model::Session, GenkaiPointDatabase},
        },
        db::mongodb::model::{MongoMessageAlias, MongoSession},
    },
    anyhow::{bail, Context as _, Result},
    async_trait::async_trait,
    mongodb::{
        bson::{self, doc},
        options::ClientOptions,
        Client, Database,
    },
    tokio_stream::StreamExt,
};

pub(crate) struct MongoDb {
    inner: Database,
}

impl MongoDb {
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
impl MessageAliasDatabase for MongoDb {
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
            .aggregate(Some(doc! { "$count": "key" }), None)
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

const GENKAI_POINT_COLLECTION_NAME: &str = "GenkaiPoint";

#[async_trait]
impl GenkaiPointDatabase for MongoDb {
    async fn create_new_session(
        &mut self,
        user_id: u64,
        joined_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool> {
        let already_have_unclosed_session = self
            .unclosed_session_exists(user_id)
            .await
            .context("failed to check that user already has unclosed session")?;

        if already_have_unclosed_session {
            return Ok(false);
        }

        let session: MongoSession = Session {
            user_id,
            joined_at,
            left_at: None,
        }
        .into();

        let doc = bson::to_document(&session).context("failed to serialize session")?;

        self.inner
            .collection(GENKAI_POINT_COLLECTION_NAME)
            .insert_one(doc, None)
            .await
            .context("failed to insert document")?;

        Ok(true)
    }

    async fn unclosed_session_exists(&self, user_id: u64) -> Result<bool> {
        let exists = self
            .inner
            .collection(GENKAI_POINT_COLLECTION_NAME)
            .aggregate(
                Some(doc! {
                    "$match": {
                        "user_id": user_id.to_string(),
                        "left_at": { "$exists": false },
                    }
                }),
                None,
            )
            .await
            .context("failed to aggregate")?
            .next()
            .await
            .is_some();

        Ok(exists)
    }

    async fn close_session(
        &mut self,
        user_id: u64,
        left_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let collection = self.inner.collection(GENKAI_POINT_COLLECTION_NAME);

        let result = collection
            .find_one_and_update(
                doc! {
                    "user_id": user_id.to_string(),
                    "left_at": { "$exists": false }
                },
                doc! { "$set": { "left_at": left_at } },
                None,
            )
            .await
            .context("failed to find unclosed session and overwrite left_at field")?;

        if result.is_none() {
            bail!("user({}) has no unclosed session", user_id);
        }

        Ok(())
    }

    async fn get_all_closed_sessions(&self, user_id: u64) -> Result<Vec<Session>> {
        self.inner
            .collection(GENKAI_POINT_COLLECTION_NAME)
            .find(
                doc! {
                    "user_id": user_id.to_string(),
                    "left_at": { "$exists": true }
                },
                None,
            )
            .await
            .context("failed to find")?
            .collect::<Result<Vec<_>, _>>()
            .await
            .context("failed to retrieve document")?
            .into_iter()
            .map(bson::from_document)
            .map(|x| x.map(MongoSession::into))
            .collect::<Result<Vec<_>, _>>()
            .context("failed to deserialize document")
    }

    async fn get_all_users_who_has_unclosed_session(&self) -> Result<Vec<u64>> {
        let list = self
            .inner
            .collection(GENKAI_POINT_COLLECTION_NAME)
            .aggregate(
                vec![
                    doc! {
                        "$match": {
                            "left_at": { "$exists": false }
                        }
                    },
                    doc! {
                        "$project": {
                            "_id": false,
                            "user_id": true
                        }
                    },
                ],
                None,
            )
            .await
            .context("failed to aggregate")?
            .collect::<Result<Vec<_>, _>>()
            .await
            .context("failed to retrieve document")?
            .into_iter()
            .map(|x| {
                x.get_str("user_id")
                    .context("this aggregation must return user_id")?
                    .parse()
                    .context("user_id must be valid number")
            })
            .collect::<Result<_, _>>()?;

        Ok(list)
    }
}
