use std::time::Duration;

use entity::sea_orm_active_enums::SubscriptionStatus;
use futures::future::join_all;
use tokio::task::JoinHandle;
use tokio::time::interval;

use crate::model::daily_email_summary::DailyEmailSentStatus;
use crate::model::user::UserCtrl;
use crate::prompt::priority_queue::PromptPriorityQueue;
use crate::{error::AppResult, ServerState};

use super::active_email_processors::ActiveEmailProcessorMap;
use super::daily_summary_mailer::DailySummaryMailer;

pub async fn add_users_to_processing(
    state: ServerState,
    email_processor_map: ActiveEmailProcessorMap,
) -> AppResult<()> {
    let conn = &state.conn;
    let user_accounts = UserCtrl::all_with_available_quota(conn).await?;
    tracing::info!("Adding {} users to processing", user_accounts.len());

    for user in user_accounts {
        match email_processor_map.insert_processor(user).await {
            Ok(_) => {}
            Err(err) => {
                tracing::error!("Error adding user to processing: {:?}", err);
            }
        }
    }

    Ok(())
}

pub async fn sweep_for_cancelled_subscriptions(
    state: &ServerState,
    email_processor_map: ActiveEmailProcessorMap,
) -> AppResult<()> {
    let conn = &state.conn;
    let user_accounts = UserCtrl::all_with_cancelled_subscriptions(conn).await?;

    for user in user_accounts {
        email_processor_map.cancel_processor(&user.email);
    }

    Ok(())
}

pub async fn send_user_daily_email_summary(
    state: &ServerState,
    user_id: i32,
) -> AppResult<DailyEmailSentStatus> {
    let user = UserCtrl::get_with_account_access_by_id(&state.conn, user_id).await?;

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

pub fn email_processing_map_cleanup(map: ActiveEmailProcessorMap) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(30));

        loop {
            // -- DEBUG
            // tracing::info!("Starting email processing map cleanup task");
            // -- DEBUG
            interval.tick().await;
            map.cleanup_stopped_processors();
        }
    })
}

/// This function pulls emails from the prompt priority queue and sends them to the
/// appropriate email processor for processing.
pub fn run_email_processing_loop(
    prompt_priority_queue: PromptPriorityQueue,
    email_processor_map: ActiveEmailProcessorMap,
) -> JoinHandle<()> {
    tracing::info!("Starting email processing loop...");
    let handles = (0..10).map(move |_| {
        let prompt_priority_queue = prompt_priority_queue.clone();
        let email_processor_map = email_processor_map.clone();
        tokio::spawn(async move {
            loop {
                let entry = prompt_priority_queue.pop();
                if let Some(entry) = entry {
                    let email = &entry.user_email;
                    let email_id = entry.email_id;
                    if let Some(processor) = email_processor_map.get(email) {
                        processor.process_email(email_id, entry.priority).await;
                        prompt_priority_queue.remove_from_processing(email_id);
                    }
                } else {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
        })
    });

    tokio::task::spawn(async move {
        for result in join_all(handles).await {
            if let Err(err) = result {
                tracing::error!("Email processing loop error: {:?}", err);
                panic!("Email processing loop panicked!");
            }
        }
    })
}
