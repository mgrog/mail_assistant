use crate::{
    db_core::prelude::*,
    error::{AppError, AppResult},
};
use chrono::Datelike;
use sea_orm::{DatabaseConnection, DbBackend};

pub struct UserTokenUsageStatsCtrl;

impl UserTokenUsageStatsCtrl {
    pub async fn get_usage_today(conn: &DatabaseConnection, user_email: &str) -> AppResult<i64> {
        let usage = UserTokenUsageStat::find()
            .filter(user_token_usage_stat::Column::UserEmail.eq(user_email))
            .filter(user_token_usage_stat::Column::Date.eq(chrono::Utc::now().date_naive()))
            .select_only()
            .column(user_token_usage_stat::Column::TokensConsumed)
            .one(conn)
            .await?
            .map(|usage| usage.tokens_consumed)
            .unwrap_or(0);

        Ok(usage)
    }

    pub async fn get_usage_this_month(
        conn: &DatabaseConnection,
        user_email: &str,
    ) -> AppResult<i64> {
        let now = chrono::Utc::now();
        let curr_month = now.month();
        let curr_year = now.year();

        #[derive(Debug, FromQueryResult)]
        pub struct MonthyUsage {
            total_tokens: i64,
        }

        let result = MonthyUsage::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Postgres,
            r#"
                SELECT COALESCE(SUM(tokens_consumed), 0) as total_tokens
                FROM user_token_usage_stat
                WHERE EXTRACT(MONTH FROM date) = $1
                AND EXTRACT(YEAR FROM date) = $2
                AND user_email = $3
            "#,
            [curr_month.into(), curr_year.into(), user_email.into()],
        ))
        .one(conn)
        .await?;

        let total_tokens: i64 = result.map(|row| row.total_tokens).unwrap_or(0);

        Ok(total_tokens)
    }

    pub async fn add_to_daily_quota(
        conn: &DatabaseConnection,
        user_email: &str,
        tokens: i64,
    ) -> AppResult<i64> {
        let today = chrono::Utc::now().date_naive();

        // Update the user's token usage in the database
        let existing = UserTokenUsageStat::find()
            .filter(user_token_usage_stat::Column::UserEmail.eq(user_email))
            .filter(user_token_usage_stat::Column::Date.eq(today))
            .one(conn)
            .await?;

        let inserted = if let Some(existing) = existing {
            UserTokenUsageStat::update_many()
                .filter(user_token_usage_stat::Column::Id.eq(existing.id))
                .col_expr(
                    user_token_usage_stat::Column::TokensConsumed,
                    Expr::col(user_token_usage_stat::Column::TokensConsumed).add(tokens),
                )
                .to_owned()
                .exec(conn)
                .await?;

            UserTokenUsageStat::find()
                .filter(user_token_usage_stat::Column::Id.eq(existing.id))
                .one(conn)
                .await?
                .ok_or(AppError::NotFound(
                    "Could not find updated token usage record".to_string(),
                ))?
        } else {
            let insertion = UserTokenUsageStat::insert(user_token_usage_stat::ActiveModel {
                id: ActiveValue::NotSet,
                user_email: ActiveValue::Set(user_email.to_string()),
                tokens_consumed: ActiveValue::Set(tokens),
                date: ActiveValue::NotSet,
                month: ActiveValue::NotSet,
                year: ActiveValue::NotSet,
                created_at: ActiveValue::NotSet,
                updated_at: ActiveValue::NotSet,
            })
            .exec(conn)
            .await?;

            UserTokenUsageStat::find()
                .filter(user_token_usage_stat::Column::Id.eq(insertion.last_insert_id))
                .one(conn)
                .await?
                .ok_or(AppError::NotFound(
                    "Could not find updated token usage record".to_string(),
                ))?
        };

        Ok(inserted.tokens_consumed)
    }
}
