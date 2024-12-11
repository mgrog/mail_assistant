use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use entity::sea_orm_active_enums::{CleanupAction, SubscriptionStatus};
use entity::{default_email_rule_override, processed_email, user};
use futures::future::join_all;
use sea_orm::{DatabaseConnection, JoinType};
use tokio::task::JoinHandle;
use tokio::time::interval;

use crate::db_core::prelude::*;
use crate::email::rules::UserEmailRules;
use crate::model::auto_cleanup_setting::AutoCleanupSettingCtrl;
use crate::model::custom_email_rule::CustomEmailRuleCtrl;
use crate::model::daily_email_summary::DailyEmailSentStatus;
use crate::model::default_email_rule_override::DefaultEmailRuleOverrideCtrl;
use crate::model::processed_email::ProcessedEmailCtrl;
use crate::model::user::UserCtrl;
use crate::prompt::priority_queue::PromptPriorityQueue;
use crate::rate_limiters::RateLimiters;
use crate::server_config::cfg;
use crate::{email, HttpClient};
use crate::{error::AppResult, ServerState};

use super::active_email_processors::ActiveEmailProcessorMap;
use super::client::EmailClient;
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
    let max_threads = cfg.api.prompt_limits.rate_limit_per_sec * 2 - 1;
    tracing::info!(
        "Starting email processing loop with {} threads...",
        max_threads
    );
    let handles = (0..max_threads).map(move |_| {
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

pub async fn cleanup_email(
    email_client: EmailClient,
    processed_email: processed_email::Model,
    action: CleanupAction,
) -> anyhow::Result<()> {
    match action {
        CleanupAction::Nothing => {}
        CleanupAction::Delete => {
            email_client.trash_email(&processed_email.id).await?;
        }
        CleanupAction::Archive => {
            email_client.archive_email(&processed_email.id).await?;
        }
    }

    Ok(())
}

async fn get_email_client(
    http_client: HttpClient,
    conn: DatabaseConnection,
    user_id: i32,
) -> anyhow::Result<EmailClient> {
    let user = UserCtrl::get_with_account_access_by_id(&conn, user_id).await?;
    let client = EmailClient::new(http_client, conn, user).await?;
    Ok(client)
}

pub async fn run_auto_cleanup(
    http_client: HttpClient,
    conn: DatabaseConnection,
) -> anyhow::Result<()> {
    let active_user_cleanup_settings =
        AutoCleanupSettingCtrl::all_active_user_cleanup_settings(&conn)
            .await
            .context("Failed to fetch cleanup settings in auto cleanup")?;

    let user_to_cleanup_settings = active_user_cleanup_settings.into_iter().fold(
        HashMap::<_, Vec<_>>::new(),
        |mut acc, setting| {
            acc.entry(setting.user_id).or_default().push(setting);
            acc
        },
    );

    for (user_id, settings) in user_to_cleanup_settings {
        let email_client = match get_email_client(http_client.clone(), conn.clone(), user_id).await
        {
            Ok(client) => client,
            Err(err) => {
                tracing::error!(
                    "Failed to create email client for user {}: {:?}",
                    user_id,
                    err
                );
                // TODO: Notify user if access issue
                continue;
            }
        };

        for setting in settings {
            if let Ok(emails_to_cleanup) =
                ProcessedEmailCtrl::get_users_processed_emails_for_cleanup(&conn, &setting).await
            {
                if emails_to_cleanup.is_empty() {
                    continue;
                }
                tracing::info!(
                    "Cleaning up {} emails for user {} according to setting:\n{:?}",
                    emails_to_cleanup.len(),
                    email_client.email_address,
                    setting
                );

                let queue = Arc::new(Mutex::new(emails_to_cleanup));

                let threads = (0..5)
                    .map(|_| {
                        let queue = queue.clone();
                        let email_client = email_client.clone();
                        let setting = setting.clone();
                        tokio::spawn(async move {
                            let next = || queue.lock().unwrap().pop();
                            while let Some(email) = next() {
                                match setting.cleanup_action {
                                    CleanupAction::Nothing => {}
                                    CleanupAction::Delete => {
                                        // -- DEBUG
                                        println!("Trashing email {:?}", email);
                                        tokio::time::sleep(Duration::from_millis(10)).await;
                                        // -- DEBUG
                                        // email_client.trash_email(&email.id).await.unwrap_or_else(
                                        //     |e| {
                                        //         tracing::error!(
                                        //             "Failed to trash email {} for user {}: {:?}",
                                        //             email.id,
                                        //             email_client.email_address,
                                        //             e
                                        //         )
                                        //     },
                                        // );
                                    }
                                    CleanupAction::Archive => {
                                        // -- DEBUG
                                        println!("Archiving email: {:?}", email);
                                        tokio::time::sleep(Duration::from_millis(10)).await;
                                        // -- DEBUG
                                        // email_client.archive_email(&email.id).await.unwrap_or_else(
                                        //     |e| {
                                        //         tracing::error!(
                                        //             "Failed to archive email {} for user {}: {:?}",
                                        //             email.id,
                                        //             email_client.email_address,
                                        //             e
                                        //         )
                                        //     },
                                        // );
                                    }
                                }
                            }
                        })
                    })
                    .collect::<tokio::task::JoinSet<_>>();

                threads.join_all().await;
            }
        }
    }
    Ok(())
}

pub fn watch(
    prompt_priority_queue: PromptPriorityQueue,
    email_processor_map: ActiveEmailProcessorMap,
    rate_limiters: RateLimiters,
) -> JoinHandle<()> {
    let mut interval = interval(Duration::from_secs(5));
    let mut now = std::time::Instant::now();
    let mut last_recorded = 0;
    tokio::spawn(async move {
        loop {
            interval.tick().await;
            let diff = email_processor_map.total_emails_processed() - last_recorded;
            let emails_per_second = diff as f64 / now.elapsed().as_secs_f64();
            now = std::time::Instant::now();
            last_recorded = email_processor_map.total_emails_processed();
            let limiter_status = rate_limiters.get_status();
            let in_processing = prompt_priority_queue.num_in_processing();
            if let Some(update) = email_processor_map.get_current_state() {
                tracing::info!(
                        "Processor Status Update:\n{email_per_second:.2} emails/s Bucket {limiter_status} Processing {in_processing}\n{update}",
                        email_per_second = emails_per_second,
                        limiter_status = limiter_status,
                        in_processing = in_processing,
                        update = update
                    );
            }
        }
    })
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_auto_cleanup() {}
}
