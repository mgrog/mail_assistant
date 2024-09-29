use entity::user_session;
use futures::future::join_all;
use sea_orm::EntityTrait;

use crate::{structs::error::AppError, ServerState};

use super::EmailProcessor;
use user_session::Entity as UserSession;

pub async fn process_emails(state: ServerState) -> anyhow::Result<()> {
    let conn = &state.conn;
    let user_sessions: Vec<_> = UserSession::find().all(conn).await?;
    let mut tasks = vec![];
    for user_session in user_sessions {
        let state = state.clone();
        let task = async {
            let processor = EmailProcessor::new(state, user_session).await?;
            processor.process().await?;

            Ok::<(), AppError>(())
        };
        tasks.push(task);
    }

    join_all(tasks).await;

    Ok(())
}
