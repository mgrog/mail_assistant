use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

pub const GMAIL_CATEGORY_LABELS: [&str; 4] = [
    "CATEGORY_PERSONAL",
    "CATEGORY_SOCIAL",
    "CATEGORY_PROMOTIONS",
    "CATEGORY_UPDATES",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AccessScopes {
    AllGmail,
    AddonsCompose,
    AddonsAction,
    AddonsMetadata,
    AddonsReadonly,
    Compose,
    Insert,
    Labels,
    Metadata,
    Modify,
    Readonly,
    Send,
    SettingsBasic,
    SettingsSharing,
}

impl FromStr for AccessScopes {
    type Err = AccessScopesParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "https://mail.google.com/" => Ok(AccessScopes::AllGmail),
            "https://www.googleapis.com/auth/gmail.addons.current.action.compose" => {
                Ok(AccessScopes::AddonsCompose)
            }
            "https://www.googleapis.com/auth/gmail.addons.current.message.action" => {
                Ok(AccessScopes::AddonsAction)
            }
            "https://www.googleapis.com/auth/gmail.addons.current.message.metadata" => {
                Ok(AccessScopes::AddonsMetadata)
            }
            "https://www.googleapis.com/auth/gmail.addons.current.message.readonly" => {
                Ok(AccessScopes::AddonsReadonly)
            }
            "https://www.googleapis.com/auth/gmail.compose" => Ok(AccessScopes::Compose),
            "https://www.googleapis.com/auth/gmail.insert" => Ok(AccessScopes::Insert),
            "https://www.googleapis.com/auth/gmail.labels" => Ok(AccessScopes::Labels),
            "https://www.googleapis.com/auth/gmail.metadata" => Ok(AccessScopes::Metadata),
            "https://www.googleapis.com/auth/gmail.modify" => Ok(AccessScopes::Modify),
            "https://www.googleapis.com/auth/gmail.readonly" => Ok(AccessScopes::Readonly),
            "https://www.googleapis.com/auth/gmail.send" => Ok(AccessScopes::Send),
            "https://www.googleapis.com/auth/gmail.settings.basic" => {
                Ok(AccessScopes::SettingsBasic)
            }
            "https://www.googleapis.com/auth/gmail.settings.sharing" => {
                Ok(AccessScopes::SettingsSharing)
            }
            _ => Err(AccessScopesParseError),
        }
    }
}

#[derive(Debug)]
pub struct AccessScopesParseError;

impl fmt::Display for AccessScopesParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid access scope")
    }
}

impl std::error::Error for AccessScopesParseError {}
