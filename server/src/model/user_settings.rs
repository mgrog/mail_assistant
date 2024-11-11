use crate::ServerState;
use crate::{
    db_core::prelude::*,
    error::{AppError, AppResult},
};
use anyhow::anyhow;

pub struct UserSettingsCtrl;

impl UserSettingsCtrl {
    pub async fn configure_default(state: &ServerState, user_email: &str) -> AppResult<()> {
        UserSettings::insert(user_settings::ActiveModel {
            id: ActiveValue::NotSet,
            user_email: ActiveValue::set(user_email.to_string()),
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
}
