use std::time::Duration;
use std::vec;

use anyhow::anyhow;
use entity::sea_orm_active_enums::SubscriptionStatus;
use entity::{prelude::*, user};
use futures::future::join_all;
use futures::FutureExt;
use sea_orm::{entity::*, query::*, DatabaseConnection};
use tokio::task::JoinHandle;
use tokio::time::interval;

use crate::db_core::queries::{get_user_with_account_access, get_users_with_active_subscriptions};
use crate::model::daily_email_summary::DailyEmailSentStatus;
use crate::{
    error::{AppError, AppResult},
    HttpClient, ServerState,
};

use super::active_email_processors::ActiveProcessingQueue;
use super::client::EmailClient;
use super::daily_summary_mailer::DailySummaryMailer;

pub async fn process_unsynced_user_emails(
    state: ServerState,
    queue: ActiveProcessingQueue,
) -> AppResult<()> {
    let conn = &state.conn;
    let user_accounts: Vec<_> = User::find()
        .filter(user::Column::SubscriptionStatus.eq(SubscriptionStatus::Active))
        .find_also_related(UserAccountAccess)
        .all(conn)
        .await?
        .into_iter()
        .filter_map(|(_, account_access)| account_access)
        .collect();

    let tasks = user_accounts
        .into_iter()
        .map(|access| queue.add_to_processing(access.user_email.clone()))
        .collect::<Vec<_>>();

    let mut successes = vec![];
    let mut errors = vec![];
    for result in join_all(tasks).await {
        match result {
            Ok(email) => {
                successes.push(email);
            }
            Err(err) => {
                tracing::error!("Error processing emails: {:?}", err);
                errors.push(err);
            }
        }
    }

    if !errors.is_empty() {
        Err(AppError::Internal(anyhow!(
            "Task failed to sync some emails: {:?}",
            errors
        )))
    } else {
        Ok(())
    }
}

pub async fn send_user_daily_email_summary(
    state: &ServerState,
    user_id: i32,
) -> AppResult<DailyEmailSentStatus> {
    let user = get_user_with_account_access(&state.conn, user_id).await?;

    let is_subscribed = user.subscription_status == SubscriptionStatus::Active;

    if !is_subscribed {
        return Ok(DailyEmailSentStatus::NotSent(
            "User is not subscribed".to_string(),
        ));
    }

    DailySummaryMailer::new(state.conn.clone(), state.http_client.clone(), user)
        .await?
        .send()
        .await;

    Ok(DailyEmailSentStatus::Sent)
}

pub async fn subscribe_to_inboxes(
    conn: DatabaseConnection,
    http_client: HttpClient,
) -> AppResult<()> {
    let active_users = get_users_with_active_subscriptions(&conn).await?;
    let accounts_to_subscribe = active_users.len();

    let mut tasks = active_users
        .into_iter()
        .map(|user| {
            let http_client = http_client.clone();
            let conn = conn.clone();
            async move {
                let email = user.email.clone();
                let client = EmailClient::new(http_client, conn, user).await?;
                let result = client.watch_mailbox().await?;

                Ok::<_, AppError>((email, result))
            }
            .boxed()
        })
        .collect::<Vec<_>>();

    if tasks.is_empty() {
        return Err(anyhow!("No active user sessions found").into());
    }

    let mut accounts_subscribed = 0;
    const CHUNK_SIZE: usize = 10;
    for chunk in tasks.chunks_mut(CHUNK_SIZE) {
        for result in join_all(chunk).await {
            match result {
                Ok((email, result)) => {
                    accounts_subscribed += 1;
                    tracing::info!("Watch message result: {:?}", result);
                    tracing::info!("Successfully subscribed to inbox for {}", email);
                }
                Err(err) => {
                    tracing::error!("Error subscribing to inbox: {:?}", err);
                }
            }
        }
        tracing::info!(
            "Subscribed to {}/{} accounts",
            accounts_subscribed,
            accounts_to_subscribe
        );
    }

    Ok(())
}

pub fn email_processing_queue_cleanup(queue: ActiveProcessingQueue) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(5));

        loop {
            // -- DEBUG
            // tracing::info!("Starting email processing queue cleanup task");
            // -- DEBUG
            interval.tick().await;
            queue.cleanup_finished_processors().await;
        }
    })
}
