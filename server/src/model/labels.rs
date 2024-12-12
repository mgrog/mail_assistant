use serde::{Deserialize, Serialize};
use strum::EnumIter;

use crate::server_config::UNKNOWN_CATEGORY;

#[derive(Debug, Clone, Copy, EnumIter, Serialize, Deserialize)]
pub enum UtilityLabels {
    Uncategorized,
    Keep,
}

impl UtilityLabels {
    pub fn as_str(&self) -> &'static str {
        match self {
            UtilityLabels::Uncategorized => UNKNOWN_CATEGORY.mail_label.as_str(),
            UtilityLabels::Keep => "keep",
        }
    }
}
