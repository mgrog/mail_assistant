use serde::{Deserialize, Serialize};
use strum::EnumIter;

#[derive(Debug, Clone, Copy, EnumIter, Serialize, Deserialize)]
pub enum CleanupLabels {
    PendingDeletion,
    PendingArchival,
}

impl CleanupLabels {
    pub fn as_str(&self) -> &'static str {
        match self {
            CleanupLabels::PendingDeletion => "pending deletion",
            CleanupLabels::PendingArchival => "pending archival",
        }
    }
}
