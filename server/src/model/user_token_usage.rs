use crate::{db_core::prelude::*, error::AppResult};
use chrono::Datelike;
use sea_orm::{DatabaseConnection, DbBackend};

pub async fn get_usage_today(conn: &DatabaseConnection, user_email: &str) -> AppResult<i64> {
    let usage = UserTokenUsageStats::find()
        .filter(user_token_usage_stats::Column::UserEmail.eq(user_email))
        .filter(user_token_usage_stats::Column::Date.eq(chrono::Utc::now().date_naive()))
        .select_only()
        .column(user_token_usage_stats::Column::TokensConsumed)
        .one(conn)
        .await?
        .map(|usage| usage.tokens_consumed)
        .unwrap_or(0);

    Ok(usage)
}

pub async fn get_usage_this_month(conn: &DatabaseConnection, user_email: &str) -> AppResult<i64> {
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
                FROM user_token_usage_stats
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
