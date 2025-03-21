mod model;

use {
    crate::{
        bot::{
            IsUpdated,
            alias::{MessageAliasDatabase, model::MessageAlias},
            auth::GenkaiAuthDatabase,
            genkai_point::{CreateNewSessionResult, GenkaiPointDatabase, model::Session},
            meigen::{
                self, MeigenDatabase, SortDirection, SortKey,
                model::{Meigen, MeigenId},
            },
        },
        db::mongodb::model::{GenkaiAuthData, MongoMeigen, MongoMessageAlias, MongoSession},
    },
    anyhow::{Context as _, Result, bail},
    chrono::{DateTime, Duration, Utc},
    mongodb::{
        Client, Collection, Database,
        bson::{self, Document, doc, oid::ObjectId},
        options::ClientOptions,
    },
    serde::{Deserialize, de::DeserializeOwned},
    tokio_stream::StreamExt,
};

#[derive(Clone)]
pub struct MongoDb {
    inner: Database,
}

impl MongoDb {
    pub async fn new(uri: &str) -> Result<Self> {
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
const GENKAI_POINT_COLLECTION_NAME: &str = "GenkaiPoint";
const GENKAI_AUTH_COLLECTION_NAME: &str = "GenkaiAuth";
const MEIGEN_COLLECTION_NAME: &str = "Meigen";

impl MessageAliasDatabase for MongoDb {
    async fn save(&self, alias: MessageAlias) -> Result<()> {
        self.inner
            .collection::<MongoMessageAlias>(MESSAGE_ALIAS_COLLECTION_NAME)
            .insert_one(MongoMessageAlias::from(alias))
            .await
            .context("failed to insert new alias")?;

        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<MessageAlias>> {
        self.inner
            .collection::<MongoMessageAlias>(MESSAGE_ALIAS_COLLECTION_NAME)
            .find_one(doc! { "key": key })
            .await
            .map(|x| x.map(|x| x.into()))
            .context("failed to deserialize alias")
    }

    async fn get_and_increment_usage_count(&self, key: &str) -> Result<Option<MessageAlias>> {
        let result = self.get(key).await?;

        if result.is_some() {
            self.inner
                .collection::<MongoMessageAlias>(MESSAGE_ALIAS_COLLECTION_NAME)
                .update_one(doc! { "key": key }, doc! { "$inc": { "usage_count": 1 } })
                .await
                .context("failed to increment usage_count")?;
        }

        Ok(result)
    }

    async fn len(&self) -> Result<u32> {
        self.inner
            .collection::<MongoMessageAlias>(MESSAGE_ALIAS_COLLECTION_NAME)
            .aggregate(Some(doc! { "$count": "key" }))
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

    async fn delete(&self, key: &str) -> Result<IsUpdated> {
        self.inner
            .collection::<MongoMessageAlias>(MESSAGE_ALIAS_COLLECTION_NAME)
            .delete_one(doc! { "key": key })
            .await
            .context("failed to delete alias")
            .map(|x| x.deleted_count == 1)
    }

    async fn usage_count_top_n(&self, n: usize) -> Result<Vec<MessageAlias>> {
        self.inner
            .collection::<MongoMessageAlias>(MESSAGE_ALIAS_COLLECTION_NAME)
            .aggregate(vec![
                doc! { "$sort": { "usage_count": -1 } },
                doc! { "$limit": n as i64 },
            ])
            .await
            .context("failed to aggregate top usage counts")?
            .map(|x| x.map(|x| bson::from_document::<MongoMessageAlias>(x).map(|x| x.into())))
            .collect::<Result<Result<Vec<_>, _>, _>>()
            .await
            .context("failed to decode document")?
            .context("failed to decode document")
    }
}

#[derive(serde::Deserialize)]

struct SessionWithDocId {
    #[serde(rename = "_id")]
    doc_id: ObjectId,

    #[serde(flatten)]
    session: MongoSession,
}

impl MongoDb {
    async fn genkai_point_get_last_user_session(
        &self,
        user_id: u64,
    ) -> Result<Option<SessionWithDocId>> {
        self.inner
            .collection::<MongoSession>(GENKAI_POINT_COLLECTION_NAME)
            .aggregate(vec![
                doc! { "$match": { "user_id": user_id.to_string() } },
                doc! { "$sort": { "joined_at": -1 } },
                doc! { "$limit": 1 },
            ])
            .await
            .context("failed to aggregate")?
            .next()
            .await
            .pipe(|r| match r {
                Some(t) => Result::<_, anyhow::Error>::Ok(bson::from_document(t?)?),
                None => Ok(None),
            })
            .context("failed to deserialize document")
    }
}

impl GenkaiPointDatabase for MongoDb {
    async fn create_new_session(
        &self,
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
            doc_id, session, ..
        }) = last_session
        {
            let session: Session = session.into();

            if let Some(left_at) = session.left_at {
                if (Utc::now() - left_at) < Duration::minutes(5) {
                    self.inner
                        .collection::<MongoSession>(GENKAI_POINT_COLLECTION_NAME)
                        .update_one(doc! { "_id": doc_id }, doc! { "$unset": { "left_at": "" } })
                        .await
                        .context("failed to unset left_at")?;

                    return Ok(CreateNewSessionResult::SessionResumed);
                }
            }
        }

        let session: MongoSession = Session {
            user_id,
            joined_at,
            left_at: None,
        }
        .into();

        self.inner
            .collection::<MongoSession>(GENKAI_POINT_COLLECTION_NAME)
            .insert_one(session)
            .await
            .context("failed to insert document")?;

        Ok(CreateNewSessionResult::NewSessionCreated)
    }

    async fn unclosed_session_exists(&self, user_id: u64) -> Result<bool> {
        let exists = self
            .inner
            .collection::<MongoSession>(GENKAI_POINT_COLLECTION_NAME)
            .aggregate(Some(doc! {
                "$match": {
                    "user_id": user_id.to_string(),
                    "left_at": { "$exists": false },
                }
            }))
            .await
            .context("failed to aggregate")?
            .next()
            .await
            .is_some();

        Ok(exists)
    }

    async fn close_session(&self, user_id: u64, left_at: DateTime<Utc>) -> Result<()> {
        let collection = self
            .inner
            .collection::<MongoSession>(GENKAI_POINT_COLLECTION_NAME);

        let result = collection
            .find_one_and_update(
                doc! {
                    "user_id": user_id.to_string(),
                    "left_at": { "$exists": false }
                },
                doc! { "$set": { "left_at": left_at } },
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
            .collection::<MongoSession>(GENKAI_POINT_COLLECTION_NAME)
            .find(doc! { "user_id": user_id.to_string() })
            .await
            .context("failed to find")?
            .map(|x| x.map(|x| x.into()))
            .collect::<Result<_, _>>()
            .await
            .context("failed to deserialize session")
    }

    async fn get_all_users_who_has_unclosed_session(&self) -> Result<Vec<u64>> {
        self.inner
            .collection::<MongoSession>(GENKAI_POINT_COLLECTION_NAME)
            .aggregate(vec![
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
            ])
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

    async fn get_all_sessions(&self) -> Result<Vec<Session>> {
        self.inner
            .collection::<MongoSession>(GENKAI_POINT_COLLECTION_NAME)
            .find(doc! {})
            .await
            .context("failed to find")?
            .map(|x| x.map(Into::into))
            .collect::<Result<_, _>>()
            .await
            .context("failed to deserialize document")
    }
}

impl GenkaiAuthDatabase for MongoDb {
    async fn register_pgp_key(&self, user_id: u64, key: &str) -> Result<()> {
        let user_id = user_id.to_string();

        self.inner
            .collection::<GenkaiAuthData>(GENKAI_AUTH_COLLECTION_NAME)
            .find_one_and_update(
                doc! { "user_id": &user_id },
                doc! { "$set": { "pgp_pub_key": key } },
            )
            .upsert(true)
            .await
            .context("failed to upsert")?;

        Ok(())
    }

    async fn get_pgp_key(&self, user_id: u64) -> Result<Option<String>> {
        let user_id = user_id.to_string();

        self.inner
            .collection::<GenkaiAuthData>(GENKAI_AUTH_COLLECTION_NAME)
            .find_one(doc! { "user_id": &user_id })
            .await
            .context("failed to find pgp key")
            .map(|x| x.and_then(|x| x.pgp_pub_key))
    }

    async fn register_token(&self, user_id: u64, token: &str) -> Result<()> {
        let user_id = user_id.to_string();

        self.inner
            .collection::<GenkaiAuthData>(GENKAI_AUTH_COLLECTION_NAME)
            .find_one_and_update(
                doc! { "user_id": &user_id },
                doc! { "$set": { "token": token } },
            )
            .upsert(true)
            .await
            .context("failed to upsert")?;

        Ok(())
    }

    async fn revoke_token(&self, user_id: u64) -> Result<()> {
        let user_id = user_id.to_string();

        self.inner
            .collection::<GenkaiAuthData>(GENKAI_AUTH_COLLECTION_NAME)
            .find_one_and_update(
                doc! { "user_id": &user_id },
                doc! { "$unset": { "token": "" } },
            )
            .upsert(true)
            .await
            .context("failed to upsert")?;

        Ok(())
    }

    async fn get_token(&self, user_id: u64) -> Result<Option<String>> {
        let user_id = user_id.to_string();

        self.inner
            .collection::<GenkaiAuthData>(GENKAI_AUTH_COLLECTION_NAME)
            .find_one(doc! { "user_id": &user_id })
            .await
            .context("failed to find pgp key")
            .map(|x| x.and_then(|x| x.token))
    }
}

impl MongoDb {
    async fn aggregate_one<T: DeserializeOwned>(
        &self,
        collection: &Collection<impl Send + Sync>, // we don't care about what collection have.
        pipeline: impl IntoIterator<Item = Document>,
    ) -> Result<T> {
        let doc = collection
            .aggregate(pipeline)
            .await
            .context("failed to aggregate")?
            .next()
            .await
            .context("aggregate returned nothing")?
            .context("failed to decode aggregated document")?;
        bson::from_document(doc).context("failed to deserialize query result")
    }
}

impl MeigenDatabase for MongoDb {
    async fn save(
        &self,
        author: impl Into<String> + Send,
        content: impl Into<String> + Send,
    ) -> Result<Meigen> {
        let author = author.into();
        let content = content.into();

        #[derive(Deserialize)]
        struct QueryResponse {
            current_id: MeigenId,
        }

        let collection = self.inner.collection::<MongoMeigen>(MEIGEN_COLLECTION_NAME);

        // FIXME: id shouldn't be decided with this method.
        // should be: https://www.mongodb.com/basics/mongodb-auto-increment
        let current_id = self
            .aggregate_one::<QueryResponse>(
                &collection,
                [doc! {
                    "$group": {
                        "_id": "",
                        "current_id": {
                            "$max": "$id"
                        }
                    }
                }],
            )
            .await?
            .current_id;

        let meigen = Meigen {
            id: current_id.succ(),
            author,
            content,
            loved_user_id: vec![],
        };

        collection
            .insert_one(MongoMeigen::from_model(meigen.clone()))
            .await
            .context("failed to insert document")?;

        Ok(meigen)
    }

    async fn load(&self, id: MeigenId) -> Result<Option<Meigen>> {
        let Some(d) = self
            .inner
            .collection::<MongoMeigen>(MEIGEN_COLLECTION_NAME)
            .find_one(doc! { "id": id.0 })
            .await
            .context("failed to find meigen")?
        else {
            return Ok(None);
        };

        Ok(Some(
            d.into_model()
                .context("failed to convert meigen to model")?,
        ))
    }

    async fn delete(&self, id: MeigenId) -> Result<IsUpdated> {
        self.inner
            .collection::<MongoMeigen>(MEIGEN_COLLECTION_NAME)
            .delete_one(doc! { "id": id.0 })
            .await
            .context("failed to delete meigen")
            .map(|x| x.deleted_count == 1)
    }

    async fn search(&self, options: meigen::FindOptions<'_>) -> Result<Vec<Meigen>> {
        let meigen::FindOptions {
            author,
            content,
            offset,
            limit,
            sort,
            dir,
            random,
        } = options;

        let mut pipeline = vec![{
            let into_regex = |x| doc! { "$regex": format!(".*{}.*", regex::escape(x)) };
            let mut doc = Document::new();
            if let Some(author) = author {
                doc.insert("author", into_regex(author));
            }
            if let Some(content) = content {
                doc.insert("content", into_regex(content));
            }
            doc! { "$match": doc } // { $match: {} } is fine, it just matches to any document.
        }];

        if random {
            // `Randomized` skips/limits before shuffling
            pipeline.extend([
                doc! { "$skip": offset },
                doc! { "$sample": { "size": limit as u32 } }, // sample pipeline scrambles document order.
                doc! { "$limit": limit as u32 },
            ]);
        }

        let dir = match dir {
            SortDirection::Asc => 1,
            SortDirection::Desc => -1,
        };

        match sort {
            SortKey::Id => pipeline.extend([doc! { "$sort": { "id": dir } }]),
            SortKey::Love => pipeline.extend([
                doc! {
                    "$addFields": {
                        "loved_users": {
                            "$size": { "$ifNull": ["$loved_user_id", []] }
                        }
                    }
                },
                doc! { "$sort": { "loved_users": dir } },
            ]),
            SortKey::Length => pipeline.extend([
                doc! { "$addFields": { "length": { "$strLenCP": "$content" }}},
                doc! { "$sort": { "length": dir } },
            ]),
        };

        if !random {
            // `Randomized` skips/limits before shuffling
            pipeline.extend([doc! { "$skip": offset }, doc! { "$limit": limit as u32 }]);
        }

        self.inner
            .collection::<MongoMeigen>(MEIGEN_COLLECTION_NAME)
            .aggregate(pipeline)
            .await
            .context("failed to aggregate")?
            .map(|x| x.context("failed to decode document"))
            .map(|x| bson::from_document(x?).context("failed to deserialize document"))
            .map(|x| MongoMeigen::into_model(x?))
            .collect()
            .await
    }

    async fn count(&self) -> Result<u32> {
        #[derive(Deserialize)]
        struct QueryResponse {
            id: i32,
        }

        let collection = self.inner.collection::<MongoMeigen>(MEIGEN_COLLECTION_NAME);

        Ok(self
            .aggregate_one::<QueryResponse>(&collection, [doc! { "$count": "id" }])
            .await?
            .id as u32)
    }

    async fn append_loved_user(&self, id: MeigenId, loved_user_id: u64) -> Result<IsUpdated> {
        self.inner
            .collection::<MongoMeigen>(MEIGEN_COLLECTION_NAME)
            .update_one(
                doc! { "id": id.0 },
                doc! { "$addToSet": { "loved_user_id": loved_user_id.to_string() } },
            )
            .await
            .context("failed to append loved user id")
            .map(|x| x.modified_count == 1)
    }

    async fn remove_loved_user(&self, id: MeigenId, loved_user_id: u64) -> Result<IsUpdated> {
        self.inner
            .collection::<MongoMeigen>(MEIGEN_COLLECTION_NAME)
            .update_one(
                doc! { "id": id.0 },
                doc! { "$pull": { "loved_user_id": loved_user_id.to_string() } },
            )
            .await
            .context("failed to remove loved user id")
            .map(|x| x.modified_count == 1)
    }
}

trait PipelineExt {
    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R,
        Self: Sized;
}

impl<T> PipelineExt for Option<T> {
    #[inline]
    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R,
        Self: Sized,
    {
        f(self)
    }
}

impl<T, E> PipelineExt for std::result::Result<T, E> {
    #[inline]
    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R,
        Self: Sized,
    {
        f(self)
    }
}
