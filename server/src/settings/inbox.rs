use std::collections::HashMap;

use anyhow::anyhow;
use entity::prelude::*;
use futures::future::join_all;
use sea_orm::DatabaseConnection;

use crate::db_core::prelude::*;
use crate::ServerState;

#[derive(Debug, Copy, Clone)]
pub struct CategoryInboxSetting {
    pub skip_inbox: bool,
    pub mark_spam: bool,
}

pub type CategoryInboxSettingsMap = HashMap<String, CategoryInboxSetting>;

pub fn default_inbox_settings() -> CategoryInboxSettingsMap {
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

pub async fn configure_default_inbox_settings(
    state: &ServerState,
    user_session_id: i32,
) -> anyhow::Result<()> {
    let handles = [
        inbox_settings::ActiveModel {
            id: ActiveValue::NotSet,
            user_session_id: ActiveValue::Set(user_session_id),
            category: ActiveValue::Set("ads".to_string()),
            skip_inbox: ActiveValue::Set(false),
            mark_spam: ActiveValue::Set(false),
        },
        inbox_settings::ActiveModel {
            id: ActiveValue::NotSet,
            user_session_id: ActiveValue::Set(user_session_id),
            category: ActiveValue::Set("political".to_string()),
            skip_inbox: ActiveValue::Set(false),
            mark_spam: ActiveValue::Set(false),
        },
        inbox_settings::ActiveModel {
            id: ActiveValue::NotSet,
            user_session_id: ActiveValue::Set(user_session_id),
            category: ActiveValue::Set("notices".to_string()),
            skip_inbox: ActiveValue::Set(false),
            mark_spam: ActiveValue::Set(false),
        },
        inbox_settings::ActiveModel {
            id: ActiveValue::NotSet,
            user_session_id: ActiveValue::Set(user_session_id),
            category: ActiveValue::Set("receipts".to_string()),
            skip_inbox: ActiveValue::Set(false),
            mark_spam: ActiveValue::Set(false),
        },
        inbox_settings::ActiveModel {
            id: ActiveValue::NotSet,
            user_session_id: ActiveValue::Set(user_session_id),
            category: ActiveValue::Set("security_alerts".to_string()),
            skip_inbox: ActiveValue::Set(false),
            mark_spam: ActiveValue::Set(false),
        },
        inbox_settings::ActiveModel {
            id: ActiveValue::NotSet,
            user_session_id: ActiveValue::Set(user_session_id),
            category: ActiveValue::Set("flights".to_string()),
            skip_inbox: ActiveValue::Set(false),
            mark_spam: ActiveValue::Set(false),
        },
        inbox_settings::ActiveModel {
            id: ActiveValue::NotSet,
            user_session_id: ActiveValue::Set(user_session_id),
            category: ActiveValue::Set("finances".to_string()),
            skip_inbox: ActiveValue::Set(false),
            mark_spam: ActiveValue::Set(false),
        },
        inbox_settings::ActiveModel {
            id: ActiveValue::NotSet,
            user_session_id: ActiveValue::Set(user_session_id),
            category: ActiveValue::Set("social_media".to_string()),
            skip_inbox: ActiveValue::Set(false),
            mark_spam: ActiveValue::Set(false),
        },
    ]
    .into_iter()
    .map(|model| {
        let state = state.clone();
        tokio::spawn(async move {
            InboxSettings::insert(model)
                .on_conflict_do_nothing()
                .exec(&state.conn)
                .await
        })
    });

    let results = join_all(handles).await;

    for result in results {
        let result = result.map_err(|_| anyhow!("Join handle error"))?;
        match result {
            Ok(_) => Ok(()),
            Err(DbErr::Query(RuntimeErr::SqlxError(error))) => match error {
                sqlx::Error::Database(error) if error.code().unwrap() == "23505" => {
                    // Ignore this error
                    Ok(())
                }
                _ => Err(anyhow!("Database error: {:?}", error)),
            },
            Err(e) => Err(e.into()),
        }?;
    }

    Ok(())
}

pub async fn get_user_inbox_settings(
    conn: &DatabaseConnection,
    user_session_id: i32,
) -> anyhow::Result<CategoryInboxSettingsMap> {
    let settings = InboxSettings::find()
        .filter(inbox_settings::Column::UserSessionId.eq(user_session_id))
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
