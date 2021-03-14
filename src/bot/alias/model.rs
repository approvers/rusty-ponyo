use {
    chrono::{DateTime, Utc},
    serde::Serialize,
};

#[derive(Clone, Serialize)]
pub(crate) struct MessageAlias {
    pub(crate) key: String,
    pub(crate) message: String,
    pub(crate) attachments: Vec<MessageAliasAttachment>,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Clone, Serialize)]
pub(crate) struct MessageAliasAttachment {
    pub(crate) name: String,
    pub(crate) data: Vec<u8>,
}
