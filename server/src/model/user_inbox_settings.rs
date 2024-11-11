use crate::{db_core::prelude::*, error::AppResult};
use std::collections::HashMap;

#[derive(Debug, Copy, Clone)]
pub struct CategoryInboxSetting {
    pub skip_inbox: bool,
    pub mark_spam: bool,
}

pub type CategoryInboxSettingsMap = HashMap<String, CategoryInboxSetting>;

pub struct UserInboxSettingsCtrl;

impl UserInboxSettingsCtrl {
    pub async fn get(
        conn: &DatabaseConnection,
        user_id: i32,
    ) -> anyhow::Result<CategoryInboxSettingsMap> {
        let settings = InboxSettings::find()
            .filter(inbox_settings::Column::UserId.eq(user_id))
            .all(conn)
            .await?
            .into_iter()
            .map(|cs| {
                (
                    cs.category,
                    CategoryInboxSetting {
                        skip_inbox: cs.skip_inbox,
                        mark_spam: cs.mark_spam,
                    },
                )
            })
            .collect();

        Ok(settings)
    }

    pub fn default() -> CategoryInboxSettingsMap {
        HashMap::from([
            (
                "ads".to_string(),
                CategoryInboxSetting {
                    skip_inbox: false,
                    mark_spam: false,
                },
            ),
            (
                "political".to_string(),
                CategoryInboxSetting {
                    skip_inbox: false,
                    mark_spam: false,
                },
            ),
            (
                "notices".to_string(),
                CategoryInboxSetting {
                    skip_inbox: false,
                    mark_spam: false,
                },
            ),
            (
                "receipts".to_string(),
                CategoryInboxSetting {
                    skip_inbox: false,
                    mark_spam: false,
                },
            ),
            (
                "security_alerts".to_string(),
                CategoryInboxSetting {
                    skip_inbox: false,
                    mark_spam: false,
                },
            ),
            (
                "flights".to_string(),
                CategoryInboxSetting {
                    skip_inbox: false,
                    mark_spam: false,
                },
            ),
            (
                "finances".to_string(),
                CategoryInboxSetting {
                    skip_inbox: false,
                    mark_spam: false,
                },
            ),
            (
                "social_media".to_string(),
                CategoryInboxSetting {
                    skip_inbox: false,
                    mark_spam: false,
                },
            ),
        ])
    }
}
