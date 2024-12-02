#![allow(clippy::from_over_into)]

use {
    crate::bot::{
        alias::model::{MessageAlias, MessageAliasAttachment},
        genkai_point::model::Session,
        meigen::model::{Meigen, MeigenId},
    },
    anyhow::{Context as _, Result},
    mongodb::bson::{spec::BinarySubtype, Binary, DateTime},
    serde::{Deserialize, Serialize},
};

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct MongoMessageAlias {
    pub(super) key: String,
    pub(super) message: String,
    pub(super) attachments: Vec<MongoMessageAliasAttachment>,
    pub usage_count: i64,
    pub created_at: DateTime,
}

impl From<MessageAlias> for MongoMessageAlias {
    fn from(origin: MessageAlias) -> Self {
        Self {
            key: origin.key,
            message: origin.message,
            created_at: origin.created_at.into(),
            usage_count: origin.usage_count as _,
            attachments: origin.attachments.into_iter().map(|x| x.into()).collect(),
        }
    }
}

impl Into<MessageAlias> for MongoMessageAlias {
    fn into(self) -> MessageAlias {
        MessageAlias {
            key: self.key,
            message: self.message,
            created_at: self.created_at.into(),
            usage_count: self.usage_count as _,
            attachments: self.attachments.into_iter().map(|x| x.into()).collect(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct MongoMessageAliasAttachment {
    pub(super) name: String,
    pub(super) data: Binary,
}

impl From<MessageAliasAttachment> for MongoMessageAliasAttachment {
    fn from(origin: MessageAliasAttachment) -> Self {
        Self {
            name: origin.name,
            data: Binary {
                subtype: BinarySubtype::Generic,
                bytes: origin.data,
            },
        }
    }
}

impl Into<MessageAliasAttachment> for MongoMessageAliasAttachment {
    fn into(self) -> MessageAliasAttachment {
        MessageAliasAttachment {
            name: self.name,
            data: self.data.bytes,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct MongoSession {
    pub(super) user_id: String,
    pub(super) joined_at: DateTime,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) left_at: Option<DateTime>,
}

impl From<Session> for MongoSession {
    fn from(s: Session) -> Self {
        Self {
            user_id: s.user_id.to_string(),
            joined_at: DateTime::from(s.joined_at),
            left_at: s.left_at.map(DateTime::from),
        }
    }
}

impl Into<Session> for MongoSession {
    fn into(self) -> Session {
        Session {
            user_id: self.user_id.parse().expect("invalid session user_id"),
            joined_at: self.joined_at.into(),
            left_at: self.left_at.map(|x| x.into()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct GenkaiAuthData {
    pub(super) user_id: String,
    pub(super) pgp_pub_key: Option<String>,
    pub(super) token: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub(super) struct MongoMeigen {
    pub(super) id: i64,
    pub(super) author: String,
    pub(super) content: String,

    // Added in PR https://github.com/approvers/MeigenBot-Rust/pull/17
    // The attribute is for the backward compatibility.
    #[serde(default)]
    pub(super) loved_user_id: Vec<String>,
}

impl MongoMeigen {
    pub fn from_model(value: Meigen) -> Self {
        MongoMeigen {
            id: value.id.0.into(),
            author: value.author,
            content: value.content,
            loved_user_id: value
                .loved_user_id
                .into_iter()
                .map(|x| x.to_string())
                .collect(),
        }
    }

    pub fn into_model(self) -> Result<Meigen> {
        Ok(Meigen {
            id: MeigenId(
                self.id
                    .try_into()
                    .context("failed to parse id from MongoMeigen")?,
            ),
            author: self.author,
            content: self.content,
            loved_user_id: self
                .loved_user_id
                .into_iter()
                .map(|x| x.parse().unwrap())
                .collect(),
        })
    }
}
