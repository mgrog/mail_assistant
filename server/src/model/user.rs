use crate::{
    db_core::prelude::*,
    error::{AppError, AppResult},
    routes::auth,
    server_config::cfg,
    HttpClient,
};
use anyhow::{anyhow, Context};
use chrono::DateTime;
use lib_utils::crypt;
use sea_orm::DbBackend;

pub struct UserCtrl;

impl UserCtrl {
    pub async fn get_by_email(conn: &DatabaseConnection, email: &str) -> AppResult<user::Model> {
        let user = User::find()
            .filter(user::Column::Email.eq(email))
            .one(conn)
            .await
            .context("Error fetching user by email")?
            .ok_or(AppError::NotFound("User not found".to_string()))?;

        Ok(user)
    }

    pub async fn get_with_account_access_by_id(
        conn: &DatabaseConnection,
        user_id: i32,
    ) -> AppResult<UserWithAccountAccess> {
        let user = User::find()
            .filter(user::Column::Id.eq(user_id))
            .join(JoinType::InnerJoin, user::Relation::UserAccountAccess.def())
            .column_as(user_account_access::Column::Id, "user_account_access_id")
            .column_as(user_account_access::Column::AccessToken, "access_token")
            .column_as(user_account_access::Column::RefreshToken, "refresh_token")
            .column_as(user_account_access::Column::ExpiresAt, "expires_at")
            .into_model::<UserWithAccountAccess>()
            .one(conn)
            .await
            .context("Error fetching user with account access")?
            .ok_or(AppError::NotFound("User not found".to_string()))?;

        Ok(user)
    }

    pub async fn get_with_account_access_by_email(
        conn: &DatabaseConnection,
        user_email: &str,
    ) -> AppResult<UserWithAccountAccess> {
        let user = User::find()
            .filter(user::Column::Email.eq(user_email))
            .join(JoinType::InnerJoin, user::Relation::UserAccountAccess.def())
            .column_as(user_account_access::Column::Id, "user_account_access_id")
            .column_as(user_account_access::Column::AccessToken, "access_token")
            .column_as(user_account_access::Column::RefreshToken, "refresh_token")
            .column_as(user_account_access::Column::ExpiresAt, "expires_at")
            .into_model::<UserWithAccountAccess>()
            .one(conn)
            .await
            .context("Error fetching user with account access")?
            .ok_or(AppError::NotFound("User not found".to_string()))?;

        Ok(user)
    }

    pub async fn get_with_account_access_and_usage_by_email(
        conn: &DatabaseConnection,
        user_email: &str,
    ) -> AppResult<UserWithAccountAccessAndUsage> {
        let user = User::find()
            .filter(user::Column::Email.eq(user_email))
            .join(JoinType::InnerJoin, user::Relation::UserAccountAccess.def())
            .join(
                JoinType::InnerJoin,
                user::Relation::UserTokenUsageStat.def(),
            )
            .column_as(user_account_access::Column::Id, "user_account_access_id")
            .column_as(user_account_access::Column::AccessToken, "access_token")
            .column_as(user_account_access::Column::RefreshToken, "refresh_token")
            .column_as(user_account_access::Column::ExpiresAt, "expires_at")
            .column_as(
                user_token_usage_stat::Column::TokensConsumed,
                "tokens_consumed",
            )
            .into_model::<UserWithAccountAccessAndUsage>()
            .one(conn)
            .await
            .context("Error fetching user with account access")?
            .ok_or(AppError::NotFound("User not found".to_string()))?;

        Ok(user)
    }

    pub async fn all_with_active_subscriptions(
        conn: &DatabaseConnection,
    ) -> AppResult<Vec<UserWithAccountAccess>> {
        let users = User::find()
            .filter(user::Column::SubscriptionStatus.eq(SubscriptionStatus::Active))
            .join(JoinType::InnerJoin, user::Relation::UserAccountAccess.def())
            .column_as(user_account_access::Column::Id, "user_account_access_id")
            .column_as(user_account_access::Column::AccessToken, "access_token")
            .column_as(user_account_access::Column::RefreshToken, "refresh_token")
            .column_as(user_account_access::Column::ExpiresAt, "expires_at")
            .into_model::<UserWithAccountAccess>()
            .all(conn)
            .await
            .context("Error fetching users with active subscriptions")?;

        Ok(users)
    }

    pub async fn all_with_available_quota(
        conn: &DatabaseConnection,
    ) -> AppResult<Vec<UserWithAccountAccessAndUsage>> {
        let today = chrono::Utc::now().date_naive();
        let daily_quota = cfg.api.token_limits.daily_user_quota as i64;

        let raw_sql = r#"
            SELECT
                u.id,
                u.email,
                CAST(u.subscription_status AS text),
                u.last_successful_payment_at,
                u.last_payment_attempt_at,
                u.created_at,
                u.updated_at,
                uaa.id AS user_account_access_id,
                uaa.access_token,
                uaa.refresh_token,
                uaa.expires_at,
                COALESCE("user_token_usage_stat".tokens_consumed, 0) AS tokens_consumed,
                GREATEST(latest_email_rule_override.updated_at, latest_custom_email_rule.updated_at) AS last_rule_update_time
            FROM
                "user" AS u
            JOIN
                "user_account_access" AS uaa ON u.email = uaa.user_email
            LEFT JOIN
                "user_token_usage_stat" ON u.email = "user_token_usage_stat".user_email AND "user_token_usage_stat".date = $1::date
            LEFT JOIN
                (
                    SELECT
                        user_id,
                        updated_at
                    FROM
                        (
                            SELECT
                                user_id,
                                updated_at,
                                ROW_NUMBER() OVER (PARTITION BY user_id ORDER BY updated_at DESC) AS row_num
                            FROM
                                default_email_rule_override
                        ) AS subquery
                    WHERE row_num = 1
                ) AS latest_email_rule_override ON u.id = latest_email_rule_override.user_id
            LEFT JOIN
                (
                    SELECT
                        user_id,
                        updated_at
                    FROM
                        (
                            SELECT
                                user_id,
                                updated_at,
                                ROW_NUMBER() OVER (PARTITION BY user_id ORDER BY updated_at DESC) AS row_num
                            FROM
                                custom_email_rule
                        ) AS subquery
                    WHERE row_num = 1
                ) AS latest_custom_email_rule ON u.id = latest_custom_email_rule.user_id
            WHERE
                u.subscription_status = (CAST('ACTIVE' AS subscription_status))
                AND ("user_token_usage_stat".tokens_consumed < $2 OR "user_token_usage_stat".tokens_consumed IS NULL)
        "#;

        let users =
            UserWithAccountAccessAndUsage::find_by_statement(Statement::from_sql_and_values(
                DbBackend::Postgres,
                raw_sql,
                [
                    today.format("%Y-%m-%d").to_string().into(),
                    daily_quota.into(),
                ],
            ))
            .all(conn)
            .await
            .context("Error fetching users with available quota")?;

        Ok(users)
    }

    pub async fn all_with_cancelled_subscriptions(
        conn: &DatabaseConnection,
    ) -> AppResult<Vec<user::Model>> {
        let users = User::find()
            .filter(user::Column::SubscriptionStatus.eq(SubscriptionStatus::Cancelled))
            .all(conn)
            .await
            .context("Error fetching users with cancelled subscriptions")?;

        Ok(users)
    }
}

pub trait Id {
    fn id(&self) -> i32;
}

pub trait EmailAddress {
    fn email(&self) -> &str;
}

pub trait AccountAccess {
    fn get_user_account_access_id(&self) -> i32;
    fn access_token(&self) -> anyhow::Result<String>;
    fn refresh_token(&self) -> anyhow::Result<String>;
    fn get_expires_at(&self) -> DateTimeWithTimeZone;
    fn set_new_access_token(&mut self, new_access_token: &str) -> anyhow::Result<()>;
}

#[derive(FromQueryResult, Clone, Debug)]
pub struct UserWithAccountAccess {
    pub id: i32,
    pub email: String,
    pub subscription_status: SubscriptionStatus,
    pub last_successful_payment_at: Option<DateTimeWithTimeZone>,
    pub last_payment_attempt_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub user_account_access_id: i32,
    access_token: String,
    refresh_token: String,
    pub expires_at: DateTimeWithTimeZone,
}

impl Id for UserWithAccountAccess {
    fn id(&self) -> i32 {
        self.id
    }
}

impl EmailAddress for UserWithAccountAccess {
    fn email(&self) -> &str {
        &self.email
    }
}

impl AccountAccess for UserWithAccountAccess {
    fn get_user_account_access_id(&self) -> i32 {
        self.user_account_access_id
    }

    fn access_token(&self) -> anyhow::Result<String> {
        let decoded = crypt::decrypt(&self.access_token)
            .map_err(|_| anyhow!("Failed to decrypt access code for: {}", self.email))?;

        Ok(decoded)
    }

    fn refresh_token(&self) -> anyhow::Result<String> {
        let decoded = crypt::decrypt(&self.refresh_token)
            .map_err(|_| anyhow!("Failed to decrypt refresh code for: {}", self.email))
            .unwrap();

        Ok(decoded)
    }

    fn get_expires_at(&self) -> DateTimeWithTimeZone {
        self.expires_at
    }

    fn set_new_access_token(&mut self, new_access_token: &str) -> anyhow::Result<()> {
        let enc_access_token = crypt::encrypt(new_access_token)
            .map_err(|e| anyhow!("Failed to encrypt access code: {e}"))?;

        self.access_token = enc_access_token;

        Ok(())
    }
}

#[derive(FromQueryResult, Clone, Debug)]
pub struct UserWithAccountAccessAndUsage {
    pub id: i32,
    pub email: String,
    pub subscription_status: SubscriptionStatus,
    pub last_successful_payment_at: Option<DateTimeWithTimeZone>,
    pub last_payment_attempt_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub user_account_access_id: i32,
    access_token: String,
    refresh_token: String,
    pub expires_at: DateTimeWithTimeZone,
    pub tokens_consumed: i64,
    pub last_rule_update_time: Option<DateTimeWithTimeZone>,
}

impl Id for UserWithAccountAccessAndUsage {
    fn id(&self) -> i32 {
        self.id
    }
}

impl EmailAddress for UserWithAccountAccessAndUsage {
    fn email(&self) -> &str {
        &self.email
    }
}

impl AccountAccess for UserWithAccountAccessAndUsage {
    fn get_user_account_access_id(&self) -> i32 {
        self.user_account_access_id
    }

    fn access_token(&self) -> anyhow::Result<String> {
        let decoded = crypt::decrypt(&self.access_token)
            .map_err(|_| anyhow!("Failed to decrypt access code for: {}", self.email))?;

        Ok(decoded)
    }

    fn refresh_token(&self) -> anyhow::Result<String> {
        let decoded = crypt::decrypt(&self.refresh_token)
            .map_err(|_| anyhow!("Failed to decrypt refresh code for: {}", self.email))
            .unwrap();

        Ok(decoded)
    }

    fn get_expires_at(&self) -> DateTimeWithTimeZone {
        self.expires_at
    }

    fn set_new_access_token(&mut self, new_access_token: &str) -> anyhow::Result<()> {
        let enc_access_token = crypt::encrypt(new_access_token)
            .map_err(|e| anyhow!("Failed to encrypt access code: {e}"))?;

        self.access_token = enc_access_token;

        Ok(())
    }
}

async fn update_account_access(
    conn: &DatabaseConnection,
    user: &mut impl AccountAccess,
    refreshed_access_token: &str,
    expires_in: i64,
) -> anyhow::Result<()> {
    let enc_access_token = crypt::encrypt(refreshed_access_token)
        .map_err(|e| anyhow!("Failed to encrypt access code: {e}"))?;

    UserAccountAccess::update(user_account_access::ActiveModel {
        id: ActiveValue::Set(user.get_user_account_access_id()),
        access_token: ActiveValue::Set(enc_access_token),
        expires_at: ActiveValue::Set(DateTime::from(
            chrono::Utc::now() + chrono::Duration::seconds(expires_in),
        )),
        ..Default::default()
    })
    .exec(conn)
    .await?;

    user.set_new_access_token(refreshed_access_token)?;

    Ok(())
}

pub async fn get_new_token(
    http_client: &HttpClient,
    conn: &DatabaseConnection,
    user: &mut impl AccountAccess,
) -> anyhow::Result<String> {
    let access_token = user.access_token()?;
    let refresh_token = user.refresh_token()?;
    let expires_at = user.get_expires_at();

    let new_access_token = if expires_at < chrono::Utc::now() {
        let resp = auth::exchange_refresh_token(http_client, &refresh_token)
            .await
            .map_err(|e| anyhow::anyhow!("Error refreshing token: {:?}", e))?;

        update_account_access(conn, user, &resp.access_token, resp.expires_in as i64)
            .await
            .map_err(|e| anyhow::anyhow!("Error updating account access: {:?}", e))?;

        resp.access_token
    } else {
        access_token
    };

    Ok(new_access_token)
}

#[cfg(test)]
mod tests {
    use sea_orm::{Database, DbBackend};

    use crate::db_core::prelude::*;
    use crate::model::user::UserCtrl;
    use crate::server_config::cfg;

    #[tokio::test]
    #[ignore]
    async fn test_query_statement() {
        dotenvy::from_filename(".env.integration").unwrap();
        let daily_quota = cfg.api.token_limits.daily_user_quota;

        let query = User::find()
            .filter(user::Column::SubscriptionStatus.eq(SubscriptionStatus::Active))
            .filter(user_token_usage_stat::Column::TokensConsumed.lt(daily_quota as i64))
            .join(JoinType::InnerJoin, user::Relation::UserAccountAccess.def())
            .join(
                JoinType::InnerJoin,
                user::Relation::UserTokenUsageStat.def(),
            )
            .column_as(user_account_access::Column::Id, "user_account_access_id")
            .column_as(
                user_token_usage_stat::Column::TokensConsumed,
                "tokens_consumed",
            )
            .build(DbBackend::Postgres)
            .to_string();

        assert_eq!(query, "")
    }

    #[tokio::test]
    async fn test_query() {
        dotenvy::from_filename(".env.integration").unwrap();
        let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        std::env::set_var("APP_DIR", root);
        let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL is not set in .env file");
        let users = UserCtrl::all_with_available_quota(&Database::connect(db_url).await.unwrap())
            .await
            .unwrap();

        dbg!(&users);

        assert!(users
            .iter()
            .all(|u| u.tokens_consumed < cfg.api.token_limits.daily_user_quota as i64));
    }
}
