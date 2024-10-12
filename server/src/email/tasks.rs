use anyhow::anyhow;
use anyhow::Context;
use entity::{prelude::*, user_session};
use futures::future::join_all;
use sea_orm::{entity::*, query::*, DatabaseConnection};

use crate::{
    structs::error::{AppError, AppResult},
    HttpClient, ServerState,
};

use super::{daily_summary_mailer::DailySummaryMailer, processor::EmailProcessor};

pub async fn process_emails(state: ServerState) -> AppResult<()> {
    let conn = &state.conn;
    let user_sessions: Vec<_> = UserSession::find()
        .filter(user_session::Column::Active.eq(true))
        .all(conn)
        .await?;
    let tasks = user_sessions.into_iter().map(|user_session| {
        let state = state.clone();
        async {
            let email = user_session.email.clone();
            let processor = EmailProcessor::new(state, user_session).await?;
            processor
                .process_full_sync()
                .await
                .context(format!("Failed to sync emails for {}", email))?;

            Ok::<_, AppError>(email)
        }
    });

    let mut has_failure = false;
    for result in join_all(tasks).await {
        match result {
            Ok(email) => {
                tracing::info!("Successfully synced emails for {}", email);
            }
            Err(err) => {
                has_failure = true;
                tracing::error!("Error processing emails: {:?}", err);
            }
        }
    }

    if has_failure {
        Err(AppError::Internal(anyhow!(
            "Task failed to sync some emails".to_string()
        )))
    } else {
        Ok(())
    }
}

pub async fn send_daily_email_summaries(
    conn: DatabaseConnection,
    http_client: HttpClient,
) -> AppResult<()> {
    let active_user_sessions = UserSession::find()
        .filter(user_session::Column::Active.eq(true))
        .all(&conn)
        .await?;
    let mut mailer =
        DailySummaryMailer::new(conn.clone(), http_client.clone(), active_user_sessions).await?;
    mailer.send_to_all_users().await;

    Ok(())
}
