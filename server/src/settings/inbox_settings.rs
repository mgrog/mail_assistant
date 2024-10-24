use anyhow::anyhow;
use entity::prelude::*;
use futures::future::join_all;

use crate::db_core::prelude::*;
use crate::model::error::AppError;
use crate::model::error::AppResult;
use crate::ServerState;

pub async fn configure_default_inbox_settings(
    state: &ServerState,
    user_session_id: i32,
) -> AppResult<()> {
    let handles = [
        inbox_settings::ActiveModel {
            id: ActiveValue::NotSet,
            user_session_id: ActiveValue::Set(user_session_id),
            category: ActiveValue::Set("ads".to_string()),
            skip_inbox: ActiveValue::Set(true),
            mark_spam: ActiveValue::Set(false),
        },
        inbox_settings::ActiveModel {
            id: ActiveValue::NotSet,
            user_session_id: ActiveValue::Set(user_session_id),
            category: ActiveValue::Set("political".to_string()),
            skip_inbox: ActiveValue::Set(true),
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
            skip_inbox: ActiveValue::Set(true),
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
                _ => Err(AppError::Internal(anyhow!("Database error: {:?}", error))),
            },
            Err(e) => Err(e.into()),
        }?;
    }

    Ok(())
}
