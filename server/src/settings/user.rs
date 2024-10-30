use anyhow::anyhow;
use entity::prelude::*;
use entity::user_settings;

use crate::db_core::prelude::*;
use crate::model::error::AppError;
use crate::model::error::AppResult;
use crate::ServerState;

pub async fn configure_default_user_settings(
    state: &ServerState,
    user_session_id: i32,
) -> AppResult<()> {
    UserSettings::insert(user_settings::ActiveModel {
        id: ActiveValue::NotSet,
        user_session_id: ActiveValue::Set(user_session_id),
        ..Default::default()
    })
    .exec(&state.conn)
    .await
    .map_err(|e| match e {
        DbErr::Query(RuntimeErr::SqlxError(error)) => match error {
            sqlx::Error::Database(error) if error.code().unwrap() == "23505" => {
                AppError::Conflict("User settings already exists".to_string())
            }
            _ => AppError::Internal(anyhow!("Database error: {:?}", error)),
        },
        _ => e.into(),
    })?;

    Ok(())
}
