#![allow(clippy::from_over_into)]

use {
    crate::bot::{
        alias::model::{MessageAlias, MessageAliasAttachment},
        genkai_point::model::Session,
    },
    mongodb::bson::{spec::BinarySubtype, Binary, DateTime},
    serde::{Deserialize, Serialize},
};

#[derive(Serialize, Deserialize)]
pub(super) struct MongoMessageAlias {
    pub(super) key: String,
    pub(super) message: String,
    pub(super) attachments: Vec<MongoMessageAliasAttachment>,
    pub(crate) created_at: DateTime,
}

impl From<MessageAlias> for MongoMessageAlias {
    fn from(origin: MessageAlias) -> Self {
        Self {
            key: origin.key,
            message: origin.message,
            created_at: origin.created_at.into(),
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
            attachments: self.attachments.into_iter().map(|x| x.into()).collect(),
        }
    }
}

#[derive(Serialize, Deserialize)]
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

#[derive(Serialize, Deserialize)]
pub(super) struct MongoSession {
    user_id: String,
    joined_at: DateTime,

    #[serde(skip_serializing_if = "Option::is_none")]
    left_at: Option<DateTime>,
}

impl From<Session> for MongoSession {
    fn from(s: Session) -> Self {
        Self {
            user_id: s.user_id.to_string(),
            joined_at: DateTime(s.joined_at),
            left_at: s.left_at.map(DateTime),
        }
    }
}

impl Into<Session> for MongoSession {
    fn into(self) -> Session {
        Session {
            user_id: self.user_id.parse().expect("invalid session user_id"),
            joined_at: self.joined_at.0,
            left_at: self.left_at.map(|x| x.0),
        }
    }
}
