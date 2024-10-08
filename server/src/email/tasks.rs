use entity::prelude::*;
use futures::future::join_all;
use sea_orm::{DatabaseConnection, EntityTrait};

use crate::{
    structs::error::{AppError, AppResult},
    ServerState,
};

use super::{daily_summary_mailer::DailySummaryMailer, processor::EmailProcessor};

pub async fn process_emails(state: ServerState) -> AppResult<()> {
    let conn = &state.conn;
    let user_sessions: Vec<_> = UserSession::find().all(conn).await?;
    let tasks = user_sessions.into_iter().map(|user_session| {
        let state = state.clone();
        async {
            let processor = EmailProcessor::new(state, user_session).await?;
            processor.process_full_sync().await?;

            Ok::<(), AppError>(())
        }
    });

    join_all(tasks).await;

    Ok(())
}

pub async fn send_daily_email_summaries(conn: DatabaseConnection) -> AppResult<()> {
    let mut mailer = DailySummaryMailer::new(conn.clone()).await?;
    mailer.send_to_all_users().await;

    Ok(())
}
