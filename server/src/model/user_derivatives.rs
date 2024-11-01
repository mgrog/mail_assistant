use crate::db_core::prelude::*;

#[derive(FromQueryResult, Clone)]
#[allow(dead_code)]
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
