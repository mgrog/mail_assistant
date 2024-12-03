use chrono::Utc;

use crate::{db_core::prelude::*, error::AppResult};

pub struct ProcessedEmailCtrl;

impl ProcessedEmailCtrl {
    pub async fn get_processed_emails_by_user(
        conn: &DatabaseConnection,
        user_id: i32,
    ) -> AppResult<Vec<processed_email::Model>> {
        let processed_emails = ProcessedEmail::find()
            .filter(processed_email::Column::UserId.eq(user_id))
            .all(conn)
            .await?;

        Ok(processed_emails)
    }

    pub async fn get_processed_emails_for_cleanup(
        conn: &DatabaseConnection,
        user_id: i32,
        timestamp: chrono::DateTime<Utc>,
        categories: Vec<String>,
    ) -> AppResult<Vec<ProcessedEmailIdCategoryAndTimestamp>> {
        let processed_emails = ProcessedEmail::find()
            .filter(processed_email::Column::UserId.eq(user_id))
            .filter(processed_email::Column::Category.is_in(categories))
            .filter(processed_email::Column::ProcessedAt.lt(timestamp))
            .select_only()
            .column(processed_email::Column::Id)
            .column(processed_email::Column::Category)
            .column(processed_email::Column::ProcessedAt)
            .into_model()
            .all(conn)
            .await?;

        Ok(processed_emails)
    }

    pub async fn insert(
        conn: &DatabaseConnection,
        active_model: processed_email::ActiveModel,
    ) -> Result<InsertResult<processed_email::ActiveModel>, DbErr> {
        ProcessedEmail::insert(active_model).exec(conn).await
    }
}

#[derive(Debug, Clone, FromQueryResult)]
pub struct ProcessedEmailIdCategoryAndTimestamp {
    pub id: String,
    pub category: String,
    pub processed_at: chrono::DateTime<Utc>,
}
