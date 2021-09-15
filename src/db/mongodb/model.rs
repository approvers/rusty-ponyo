#![allow(clippy::from_over_into)]

use {
    crate::bot::{
        alias::model::{MessageAlias, MessageAliasAttachment},
        genkai_point::model::Session,
    },
    mongodb::bson::{spec::BinarySubtype, Binary, DateTime},
    serde::{Deserialize, Serialize},
};

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct MongoMessageAlias {
    pub(super) key: String,
    pub(super) message: String,
    pub(super) attachments: Vec<MongoMessageAliasAttachment>,
    pub(crate) usage_count: i64,
    pub(crate) created_at: DateTime,
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
