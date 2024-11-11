use crate::{db_core::prelude::*, routes::auth, HttpClient};
use anyhow::anyhow;
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
