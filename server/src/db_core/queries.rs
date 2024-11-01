use anyhow::{anyhow, Context};

use crate::{
    model::{
        error::{AppError, AppResult},
        settings_derivatives::{CategoryInboxSetting, CategoryInboxSettingsMap},
        user_derivatives::UserWithAccountAccess,
    },
    ServerState,
};

use super::prelude::*;

pub async fn get_user_with_account_access(
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
        .context("User not found")?;

    Ok(user)
}

pub async fn get_user_with_account_access_by_email(
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
        .context("User not found")?;

    Ok(user)
}

pub async fn get_users_with_active_subscriptions(
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

pub async fn get_user_inbox_settings(
    conn: &DatabaseConnection,
    user_id: i32,
) -> anyhow::Result<CategoryInboxSettingsMap> {
    let settings = InboxSettings::find()
        .filter(inbox_settings::Column::UserId.eq(user_id))
        .all(conn)
        .await?
        .into_iter()
        .map(|cs| {
            (
                cs.category,
                CategoryInboxSetting {
                    skip_inbox: cs.skip_inbox,
                    mark_spam: cs.mark_spam,
                },
            )
        })
        .collect();

    Ok(settings)
}

pub async fn configure_default_user_settings(
    state: &ServerState,
    user_email: &str,
) -> AppResult<()> {
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

#[cfg(test)]
mod tests {
    use sea_orm::DbBackend;

    use super::*;
    use crate::db_core::test::setup_conn;

    #[test]
    #[ignore]
    fn test_get_user_with_account_access_query() {
        let query = User::find()
            .filter(user::Column::Id.eq(1))
            .join(JoinType::InnerJoin, user::Relation::UserAccountAccess.def())
            .column_as(user_account_access::Column::Id, "user_account_access_id")
            .column_as(user_account_access::Column::AccessToken, "access_token")
            .column_as(user_account_access::Column::RefreshToken, "refresh_token")
            .column_as(user_account_access::Column::ExpiresAt, "expires_at")
            .build(DbBackend::Postgres)
            .to_string();

        assert_eq!(query, "");
    }

    #[tokio::test]
    async fn test_get_user_with_account_access() {
        let conn = setup_conn().await;
        let user = get_user_with_account_access(&conn, 1).await.unwrap();

        assert_eq!(user.id, 1);
    }
}
