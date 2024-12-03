use crate::{db_core::prelude::*, error::AppResult};

pub struct CleanupSettingsCtrl;

impl CleanupSettingsCtrl {
    pub async fn get_user_cleanup_settings(
        conn: &DatabaseConnection,
        user_id: i32,
    ) -> AppResult<Vec<cleanup_settings::Model>> {
        let cleanup_settings = CleanupSettings::find()
            .filter(cleanup_settings::Column::UserId.eq(user_id))
            .all(conn)
            .await?;

        Ok(cleanup_settings)
    }
}
