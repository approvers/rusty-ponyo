mod model;

use {
    crate::{
        bot::{
            alias::{model::MessageAlias, MessageAliasDatabase},
            genkai_point::{
                model::{Session, UserStat},
                CreateNewSessionResult, GenkaiPointDatabase,
            },
        },
        db::mongodb::model::{MongoMessageAlias, MongoSession},
    },
    anyhow::{bail, Context as _, Result},
    async_trait::async_trait,
    chrono::{DateTime, Duration, Utc},
    hashbrown::HashMap,
    mongodb::{
        bson::{self, doc, oid::ObjectId},
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
        self.inner
            .collection_with_type::<MongoMessageAlias>(MESSAGE_ALIAS_COLLECTION_NAME)
            .insert_one(alias.into(), None)
            .await
            .context("failed to insert new alias")?;

        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<MessageAlias>> {
        self.inner
            .collection_with_type::<MongoMessageAlias>(MESSAGE_ALIAS_COLLECTION_NAME)
            .find_one(doc! { "key": key }, None)
            .await
            .map(|x| x.map(|x| x.into()))
            .context("failed to deserialize alias")
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

struct SessionWithDocId {
    doc_id: ObjectId,
    session: Session,
}

impl MongoDb {
    async fn genkai_point_get_last_user_session(
        &self,
        user_id: u64,
    ) -> Result<Option<SessionWithDocId>> {
        self.inner
            .collection(GENKAI_POINT_COLLECTION_NAME)
            .aggregate(
                vec![
                    doc! { "$match": { "user_id": user_id.to_string() } },
                    doc! { "$sort": { "joined_at": -1 } },
                ],
                None,
            )
            .await
            .context("failed to aggregate")?
            .next()
            .await
            .map(|x| {
                x.context("failed to deserialize document").and_then(|x| {
                    let did = x
                        .get_object_id("_id")
                        .context("Document ID (_id field) not found")?
                        .clone();

                    bson::from_document::<MongoSession>(x)
                        .map(|x| SessionWithDocId {
                            doc_id: did,
                            session: x.into(),
                        })
                        .context("failed to deserialize document")
                })
            })
            .transpose()
    }
}

#[async_trait]
impl GenkaiPointDatabase for MongoDb {
    async fn create_new_session(
        &mut self,
        user_id: u64,
        joined_at: DateTime<Utc>,
    ) -> Result<CreateNewSessionResult> {
        let already_have_unclosed_session = self
            .unclosed_session_exists(user_id)
            .await
            .context("failed to check that user already has unclosed session")?;

        if already_have_unclosed_session {
            return Ok(CreateNewSessionResult::UnclosedSessionExists);
        }

        let last_session = self
            .genkai_point_get_last_user_session(user_id)
            .await
            .context("failed to get last session")?;

        if let Some(SessionWithDocId {
            doc_id,
            session:
                Session {
                    left_at: Some(left_at),
                    ..
                },
            ..
        }) = last_session
        {
            if (Utc::now() - left_at) < Duration::minutes(5) {
                self.inner
                    .collection_with_type::<MongoSession>(GENKAI_POINT_COLLECTION_NAME)
                    .update_one(
                        doc! { "_id": doc_id },
                        doc! { "$unset": { "left_at": "" } },
                        None,
                    )
                    .await
                    .context("failed to unset left_at")?;

                return Ok(CreateNewSessionResult::SessionResumed);
            }
        }

        let session: MongoSession = Session {
            user_id,
            joined_at,
            left_at: None,
        }
        .into();

        self.inner
            .collection_with_type::<MongoSession>(GENKAI_POINT_COLLECTION_NAME)
            .insert_one(session, None)
            .await
            .context("failed to insert document")?;

        Ok(CreateNewSessionResult::CreatedNewSession)
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

    async fn close_session(&mut self, user_id: u64, left_at: DateTime<Utc>) -> Result<()> {
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

    async fn get_users_all_sessions(&self, user_id: u64) -> Result<Vec<Session>> {
        self.inner
            .collection_with_type::<MongoSession>(GENKAI_POINT_COLLECTION_NAME)
            .find(doc! { "user_id": user_id.to_string() }, None)
            .await
            .context("failed to find")?
            .map(|x| x.map(|x| x.into()))
            .collect::<Result<_, _>>()
            .await
            .context("failed to deserialize session")
    }

    async fn get_all_users_who_has_unclosed_session(&self) -> Result<Vec<u64>> {
        self.inner
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
            .map(|x| {
                x.context("failed to deserialize document").and_then(|x| {
                    x.get_str("user_id")
                        .context("this aggregation must return user_id")?
                        .parse::<u64>()
                        .context("user_id must be valid number")
                })
            })
            .collect::<Result<_, _>>()
            .await
            .context("failed to retrieve document")
    }

    async fn get_all_users_stats(&self) -> Result<Vec<UserStat>> {
        let mut stream = self
            .inner
            .collection_with_type::<MongoSession>(GENKAI_POINT_COLLECTION_NAME)
            .find(None, None)
            .await
            .context("failed to find")?;

        let mut user_sessions = HashMap::new();

        while let Some(session) = stream.next().await {
            let session: Session = session.context("failed to deserialize document")?.into();

            user_sessions
                .entry(session.user_id)
                .or_insert_with(Vec::new)
                .push(session);
        }

        user_sessions
            .iter()
            .flat_map(|(_, x)| UserStat::from_sessions(x).transpose())
            .collect::<Result<Vec<_>, _>>()
            .context("failed to calc userstat")
    }
}
