use crate::db_core::prelude::*;
use sea_orm::DatabaseConnection;

pub struct AutoCleanupSettingCtrl;

impl AutoCleanupSettingCtrl {
    pub async fn all_active_user_cleanup_settings(
        conn: &DatabaseConnection,
    ) -> anyhow::Result<Vec<auto_cleanup_setting::Model>> {
        let cleanup_settings = AutoCleanupSetting::find()
            .join(
                JoinType::InnerJoin,
                auto_cleanup_setting::Relation::User.def(),
            )
            .filter(user::Column::SubscriptionStatus.eq(SubscriptionStatus::Active))
            .all(conn)
            .await?;

        Ok(cleanup_settings)
    }
}
