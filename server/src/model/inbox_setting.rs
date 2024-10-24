use entity::prelude::*;
use sea_orm::{DerivePartialModel, FromQueryResult};

#[derive(Debug, Clone, DerivePartialModel, FromQueryResult)]
#[sea_orm(entity = "InboxSettings")]
pub struct PartialInboxSetting {
    pub category: String,
    pub skip_inbox: bool,
    pub mark_spam: bool,
}
