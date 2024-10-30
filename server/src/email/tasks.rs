use std::time::Duration;
use std::vec;

use anyhow::anyhow;
use entity::{prelude::*, user_session};
use futures::future::join_all;
use futures::FutureExt;
use sea_orm::{entity::*, query::*, DatabaseConnection};
use tokio::task::JoinHandle;
use tokio::time::interval;

use crate::model::daily_email_summary::DailyEmailSentStatus;
use crate::{
    model::error::{AppError, AppResult},
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
    let user_sessions: Vec<_> = UserSession::find()
        .filter(user_session::Column::Active.eq(true))
        // .filter(
        //     Condition::any()
        //         .add(user_session::Column::LastSync.is_null())
        //         .add(
        //             user_session::Column::LastSync
        //                 .lt(chrono::Utc::now() - chrono::Duration::hours(6)),
        //         ),
        // )
        .all(conn)
        .await?;

    let tasks = user_sessions
        .into_iter()
        .map(|user_session| queue.add_to_processing(user_session.email.clone()))
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
    user_session_id: i32,
) -> AppResult<DailyEmailSentStatus> {
    let user_session = UserSession::find()
        .filter(user_session::Column::Id.eq(user_session_id))
        .filter(user_session::Column::Active.eq(true))
        .one(&state.conn)
        .await?;

    if let Some(active_session) = user_session {
        DailySummaryMailer::new(
            state.conn.clone(),
            state.http_client.clone(),
            active_session,
        )
        .await?
        .send()
        .await;

        Ok(DailyEmailSentStatus::Sent)
    } else {
        Ok(DailyEmailSentStatus::NotSent(
            "User does not have an active session".to_string(),
        ))
    }
}

pub async fn subscribe_to_inboxes(
    conn: DatabaseConnection,
    http_client: HttpClient,
) -> AppResult<()> {
    let active_user_sessions = UserSession::find()
        .filter(user_session::Column::Active.eq(true))
        .all(&conn)
        .await?;

    let accounts_to_subscribe = active_user_sessions.len();

    let mut tasks = active_user_sessions
        .into_iter()
        .map(|user_session| {
            let http_client = http_client.clone();
            let conn = conn.clone();
            async move {
                let email = user_session.email.clone();
                let client = EmailClient::new(http_client, conn, user_session).await?;
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
