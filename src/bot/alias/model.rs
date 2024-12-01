use {
    chrono::{DateTime, Utc},
    serde::Serialize,
};

#[derive(Clone, Serialize)]
pub struct MessageAlias {
    pub key: String,
    pub message: String,
    pub attachments: Vec<MessageAliasAttachment>,
    pub usage_count: u32,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Serialize)]
pub struct MessageAliasAttachment {
    pub name: String,
    pub data: Vec<u8>,
}
