use {
    crate::model::{MessageAlias, MessageAliasAttachment},
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
