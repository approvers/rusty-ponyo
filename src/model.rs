use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub(crate) struct MessageAlias {
    pub(crate) key: String,
    pub(crate) message: String,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct MessageAliasRef<'a> {
    pub(crate) key: &'a str,
    pub(crate) message: &'a str,
}
