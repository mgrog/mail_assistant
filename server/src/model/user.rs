use crate::{db_core::prelude::*, error::AppResult, routes::auth, HttpClient};
use anyhow::{anyhow, Context};
use chrono::DateTime;
use lib_utils::crypt;

#[derive(FromQueryResult, Clone, Debug)]
pub struct UserWithAccountAccess {
    pub id: i32,
    pub email: String,
    pub subscription_status: SubscriptionStatus,
    pub last_successful_payment_at: Option<DateTimeWithTimeZone>,
    pub last_payment_attempt_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub last_sync: Option<DateTimeWithTimeZone>,
    pub user_account_access_id: i32,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTimeWithTimeZone,
}

impl UserWithAccountAccess {
    pub async fn get_valid_access_code(
        &mut self,
        conn: &DatabaseConnection,
        http_client: HttpClient,
    ) -> anyhow::Result<String> {
        let access_token = if self.expires_at < chrono::Utc::now() {
            let resp = auth::exchange_refresh_token(http_client.clone(), &self.refresh_token)
                .await
                .map_err(|e| anyhow::anyhow!("Error refreshing token: {:?}", e))?;

            self._update_account_access(conn, &resp.access_token, resp.expires_in as i64)
                .await
                .map_err(|e| anyhow::anyhow!("Error updating account access: {:?}", e))?;

            self.access_token.clone()
        } else {
            self.access_token.clone()
        };

        Ok(access_token)
    }

    async fn _update_account_access(
        &mut self,
        conn: &DatabaseConnection,
        refreshed_access_code: &str,
        expires_in: i64,
    ) -> anyhow::Result<()> {
        let enc_access_code = crypt::encrypt(refreshed_access_code)
            .map_err(|e| anyhow!("Failed to encrypt access code: {e}"))?;

        UserAccountAccess::update(user_account_access::ActiveModel {
            id: ActiveValue::Set(self.user_account_access_id),
            access_token: ActiveValue::Set(enc_access_code),
            expires_at: ActiveValue::Set(DateTime::from(
                chrono::Utc::now() + chrono::Duration::seconds(expires_in),
            )),
            ..Default::default()
        })
        .exec(conn)
        .await?;

        self.access_token = refreshed_access_code.to_string();

        Ok(())
    }
}

pub struct UserCtrl;

impl UserCtrl {
    pub async fn get_with_account_access_by_id(
        conn: &DatabaseConnection,
        user_id: i32,
    ) -> AppResult<UserWithAccountAccess> {
        let mut user = User::find()
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
            .context("User not found")?;

        user.access_token = crypt::decrypt(&user.access_token)?;
        user.refresh_token = crypt::decrypt(&user.refresh_token)?;

        Ok(user)
    }

    pub async fn get_with_account_access_by_email(
        conn: &DatabaseConnection,
        user_email: &str,
    ) -> AppResult<UserWithAccountAccess> {
        let mut user = User::find()
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
            .context("User not found")?;

        user.access_token = crypt::decrypt(&user.access_token)?;
        user.refresh_token = crypt::decrypt(&user.refresh_token)?;

        Ok(user)
    }

    pub async fn all_with_active_subscriptions(
        conn: &DatabaseConnection,
    ) -> AppResult<Vec<UserWithAccountAccess>> {
        let users = User::find()
            .filter(user::Column::SubscriptionStatus.eq(SubscriptionStatus::Active))
            .join(JoinType::InnerJoin, user::Relation::UserAccountAccess.def())
            .column_as(user_account_access::Column::Id, "user_account_access_id")
            .into_model::<UserWithAccountAccess>()
            .all(conn)
            .await
            .context("Error fetching users with active subscriptions")?;

        Ok(users)
    }
}
