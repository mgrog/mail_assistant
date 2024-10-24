use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicI64},
        Arc,
    },
};

use anyhow::Context;
use derive_more::Display;
use entity::{email_training, prelude::*, processed_email, user_session, user_token_usage_stats};
use futures::future::join_all;
use queue::{EmailProcessingQueue, EmailQueueStatus};
use sea_orm::{
    entity::*, prelude::Expr, query::*, sea_query::OnConflict, ActiveValue, EntityTrait,
    FromQueryResult,
};
use std::sync::atomic::Ordering::Relaxed;

use crate::{
    email::client::EmailClient,
    model::error::{AppError, AppResult},
    server_config::{cfg, UNKNOWN_CATEGORY},
    settings::prelude::*,
    ServerState,
};
use crate::{
    email::client::MessageListOptions,
    prompt::mistral::{self, CategoryPromptResponse},
};

use super::client::EmailMessage;

mod queue;

#[derive(Display)]
pub enum ProcessorStatus {
    Processing,
    Completed,
    Cancelled,
    QuotaExceeded,
}

#[derive(Display)]
#[display("status:{status}, emails_processed:{emails_processed}, emails_failed:{emails_failed}, emails_remaining:{emails_remaining}")]
pub struct EmailProcessorStatusUpdate {
    pub status: ProcessorStatus,
    pub emails_processed: i64,
    pub emails_failed: i64,
    pub emails_remaining: i64,
}

// This struct processes emails for a single user
#[derive(Clone)]
pub struct EmailProcessor {
    pub user_session_id: i32,
    pub email_address: String,
    processed_email_count: Arc<AtomicI64>,
    failed_email_count: Arc<AtomicI64>,
    email_client: Arc<EmailClient>,
    server_state: ServerState,
    token_count: Arc<AtomicI64>,
    cancelled: Arc<AtomicBool>,
    email_queue: EmailProcessingQueue,
    remaining_quota: i64,
}

impl EmailProcessor {
    pub async fn new(
        server_state: ServerState,
        user_session: user_session::Model,
    ) -> AppResult<Self> {
        let email_client = EmailClient::new(
            server_state.http_client.clone(),
            server_state.conn.clone(),
            user_session.clone(),
        )
        .await
        .context(format!(
            "Could not create email client for: {}",
            user_session.email
        ))?;

        tracing::info!(
            "Email client created successfully for {}",
            user_session.email
        );

        let quota_used = UserTokenUsageStats::find()
            .filter(user_token_usage_stats::Column::UserSessionId.eq(user_session.id))
            .filter(user_token_usage_stats::Column::Date.eq(chrono::Utc::now().date_naive()))
            .one(&server_state.conn)
            .await?
            .map(|usage| usage.tokens_consumed)
            .unwrap_or(0);

        let remaining_quota = cfg.api.token_limits.daily_user_quota as i64 - quota_used;
        // -- DEBUG
        // println!("User's current usage: {}", quota_used);
        // println!("User's remaining quota: {}", remaining_quota);
        // -- DEBUG

        let processor = EmailProcessor {
            user_session_id: user_session.id,
            email_address: user_session.email,
            processed_email_count: Arc::new(AtomicI64::new(0)),
            failed_email_count: Arc::new(AtomicI64::new(0)),
            server_state,
            email_client: Arc::new(email_client),
            token_count: Arc::new(AtomicI64::new(0)),
            cancelled: Arc::new(AtomicBool::new(false)),
            email_queue: EmailProcessingQueue::new(),
            remaining_quota,
        };

        Ok(processor)
    }

    pub async fn from_email_address(
        server_state: ServerState,
        email_address: &str,
    ) -> AppResult<Self> {
        let user_session = UserSession::find()
            .filter(user_session::Column::Email.eq(email_address))
            .filter(user_session::Column::Active.eq(true))
            .one(&server_state.conn)
            .await?
            .context("Could not find active user session")?;

        Self::new(server_state, user_session).await
    }

    pub async fn update_last_sync_time(&self) -> anyhow::Result<()> {
        UserSession::update(user_session::ActiveModel {
            id: ActiveValue::Set(self.user_session_id),
            last_sync: ActiveValue::Set(Some(chrono::Utc::now().into())),
            ..Default::default()
        })
        .exec(&self.server_state.conn)
        .await
        .context("Error updating last sync time")?;

        Ok(())
    }

    pub fn check_is_processing(&self) -> bool {
        !self.is_cancelled() && !self.is_quota_reached() && !self.email_queue.is_empty()
    }

    pub async fn fetch_new_email_ids(&self) -> anyhow::Result<Vec<u128>> {
        let message_list_resp = self
            .email_client
            .get_message_list(MessageListOptions {
                more_recent_than: chrono::Duration::weeks(2),
                ..MessageListOptions::default()
            })
            .await?;

        let message_ids: Vec<_> = message_list_resp
            .messages
            .as_ref()
            .map(|list| {
                list.iter()
                    .filter_map(|m| m.id.clone())
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

        #[derive(FromQueryResult)]
        #[allow(dead_code)]
        struct PartialProcessedEmail {
            id: String,
            ai_answer: String,
        }

        let already_processed_ids = Arc::new(
            processed_email::Entity::find()
                .filter(processed_email::Column::Id.is_in(&message_ids))
                .select_only()
                .column(processed_email::Column::Id)
                .column(processed_email::Column::AiAnswer)
                .into_model::<PartialProcessedEmail>()
                .all(&self.server_state.conn)
                .await?
                .into_iter()
                .map(|e| (e.id.clone(), e))
                .collect::<HashMap<_, _>>(),
        );

        let new_email_ids = message_ids
            .into_iter()
            .filter(|id| !already_processed_ids.contains_key(id))
            .map(parse_id_to_int)
            .collect::<Vec<_>>();

        Ok(new_email_ids)
    }

    pub fn add_email_ids_to_queue(&mut self, email_ids: Vec<u128>) {
        for email_id in email_ids {
            self.email_queue.add_to_queue(email_id);
        }
    }

    pub async fn queue_new_emails(&mut self) -> AppResult<()> {
        let new_email_ids = self.fetch_new_email_ids().await?;

        for email_id in new_email_ids {
            self.email_queue.add_to_queue(email_id);
        }

        Ok(())
    }

    pub async fn process_email(&self, email_message: &EmailMessage) -> AppResult<i64> {
        let email_id = &email_message.id;
        let current_labels = &email_message.label_ids;

        let CategoryPromptResponse {
            category: mut category_content,
            confidence,
            token_usage,
        } = match mistral::send_category_prompt(&self.server_state, email_message).await {
            Ok(resp) => Ok::<_, AppError>(resp),
            Err(e) => {
                tracing::error!("Error processing email {}: {:?}", email_id, e);
                return Err::<_, AppError>(e);
            }
        }?;

        let mut email_training: Option<email_training::ActiveModel> = None;
        if cfg.settings.training_mode {
            email_training = Some(email_training::ActiveModel {
                id: ActiveValue::NotSet,
                user_email: ActiveValue::Set(self.email_address.clone()),
                email_id: ActiveValue::Set(email_id.clone()),
                from: ActiveValue::Set(email_message.from.clone().unwrap_or_default()),
                subject: ActiveValue::Set(email_message.subject.clone().unwrap_or_default()),
                body: ActiveValue::Set(email_message.body.clone().unwrap_or_default()),
                ai_answer: ActiveValue::Set(category_content.clone()),
                confidence: ActiveValue::Set(confidence),
                heuristics_used: ActiveValue::NotSet,
            });
        }

        if confidence < cfg.model.email_confidence_threshold {
            category_content = "Unknown".to_string();
        }

        let email_category = cfg
            .categories
            .iter()
            .find(|c| c.content == category_content)
            .unwrap_or(&UNKNOWN_CATEGORY);

        let email_category = match email_message.from.as_ref() {
            // If the category is a heuristic category, apply the corresponding label
            // Heuristic categories use identifiers from common companies to label emails
            // It can help label certain emails that are not easily categorized by the AI
            Some(from)
                if email_category.content != "Terms of Service Update"
                    && email_category.content != "Verification Code"
                    && email_category.content != "Security Alert"
                    && cfg.heuristics.iter().any(|c| from.contains(&c.content)) =>
            {
                if let Some(training) = email_training {
                    email_training = Some(email_training::ActiveModel {
                        heuristics_used: ActiveValue::Set(true),
                        ..training
                    });
                }
                cfg.heuristics
                    .iter()
                    .find(|c| from.contains(&c.content))
                    .unwrap()
            }
            // Otherwise, apply the category label
            _ => cfg
                .categories
                .iter()
                .find(|c| c.content == category_content)
                .unwrap_or(&UNKNOWN_CATEGORY),
        };

        if let Some(email_training) = email_training {
            EmailTraining::insert(email_training)
                .on_conflict(
                    OnConflict::column(email_training::Column::EmailId)
                        .update_columns([
                            email_training::Column::AiAnswer,
                            email_training::Column::Confidence,
                            email_training::Column::HeuristicsUsed,
                        ])
                        .to_owned(),
                )
                .exec(&self.server_state.conn)
                .await
                .context("Error inserting email training data")?;
        }

        let label_update = self
            .email_client
            .label_email(
                email_id.clone(),
                current_labels.clone(),
                email_category.clone(),
            )
            .await?;

        ProcessedEmail::insert(processed_email::ActiveModel {
            id: ActiveValue::Set(email_id.clone()),
            user_session_id: ActiveValue::Set(self.user_session_id),
            labels_applied: ActiveValue::Set(label_update.added),
            labels_removed: ActiveValue::Set(label_update.removed),
            ai_answer: ActiveValue::Set(category_content),
            processed_at: ActiveValue::NotSet,
        })
        .exec(&self.server_state.conn)
        .await?;

        Ok::<_, AppError>(token_usage)
    }

    pub async fn process_emails_in_queue(&self) -> anyhow::Result<()> {
        if self.is_quota_reached() {
            return Ok(());
        }

        tracing::info!("Processing emails for {}", self.email_address);

        match self.email_client.configure_labels_if_needed().await {
            Ok(true) => {
                tracing::info!("Labels configured successfully for {}", self.email_address);
            }
            Ok(false) => {
                tracing::info!("Labels already configured for {}", self.email_address);
            }
            Err(e) => {
                tracing::error!(
                    "Error configuring labels for {}: {:?}",
                    self.email_address,
                    e
                );
                return Err(e);
            }
        }

        match configure_default_inbox_settings(&self.server_state, self.user_session_id).await {
            Ok(_) => {
                tracing::info!(
                    "Inbox settings configured successfully for {}",
                    self.email_address
                );
            }
            Err(e) => {
                tracing::error!(
                    "Error configuring inbox settings for {}: {:?}",
                    self.email_address,
                    e
                );
                return Err(anyhow::anyhow!("{:?}", e));
            }
        }

        // Create 3 concurrent email processing threads to pull from queue
        let handles = (0..3).map(|_| {
            let queue = self.email_queue.clone();
            let self_ = self.clone();
            tokio::spawn(async move {
                while let Some(id) = queue.get_next() {
                    if self_.is_cancelled() {
                        return EmailQueueStatus::Cancelled;
                    }

                    if self_.is_quota_reached() {
                        return EmailQueueStatus::QuotaExceeded;
                    }

                    let email_id = parse_int_to_id(id);
                    let email_message =
                        match self_.email_client.get_sanitized_message(&email_id).await {
                            Ok(email_message) => email_message,
                            Err(e) => {
                                tracing::error!("Error fetching email {}: {:?}", email_id, e);
                                self_.fetch_add_total_emails_failed(1);
                                continue;
                            }
                        };

                    self_.server_state.rate_limiters.acquire_one().await;

                    match self_.process_email(&email_message).await {
                        Ok(token_usage) => {
                            self_.fetch_add_total_emails_processed(1);
                            self_.fetch_add_token_count(token_usage);
                        }
                        Err(e) => {
                            tracing::error!("Error processing email {}: {:?}", email_id, e);
                            self_.fetch_add_total_emails_failed(1);
                        }
                    }
                }

                EmailQueueStatus::Complete
            })
        });

        for result in join_all(handles).await {
            result.context("Email processing join error")?;
        }

        self.add_tally_to_user_daily_quota(self.current_token_usage())
            .await?;

        self.update_last_sync_time().await?;

        Ok(())
    }

    async fn add_tally_to_user_daily_quota(&self, tokens: i64) -> anyhow::Result<()> {
        tracing::info!(
            "Adding {} tokens to user {}'s daily quota",
            tokens,
            self.email_address
        );

        // Update the user's token usage in the database
        let existing = UserTokenUsageStats::find()
            .filter(user_token_usage_stats::Column::UserSessionId.eq(self.user_session_id))
            .filter(user_token_usage_stats::Column::Date.eq(chrono::Utc::now().date_naive()))
            .one(&self.server_state.conn)
            .await?;

        let inserted = if let Some(existing) = existing {
            UserTokenUsageStats::update_many()
                .filter(user_token_usage_stats::Column::Id.eq(existing.id))
                .col_expr(
                    user_token_usage_stats::Column::TokensConsumed,
                    Expr::col(user_token_usage_stats::Column::TokensConsumed).add(tokens),
                )
                .to_owned()
                .exec(&self.server_state.conn)
                .await?;

            UserTokenUsageStats::find()
                .filter(user_token_usage_stats::Column::Id.eq(existing.id))
                .one(&self.server_state.conn)
                .await?
                .context("Could not find updated token usage record")?
        } else {
            let insertion = UserTokenUsageStats::insert(user_token_usage_stats::ActiveModel {
                id: ActiveValue::NotSet,
                user_session_id: ActiveValue::Set(self.user_session_id),
                tokens_consumed: ActiveValue::Set(tokens),
                date: ActiveValue::NotSet,
                created_at: ActiveValue::NotSet,
                updated_at: ActiveValue::NotSet,
            })
            .exec(&self.server_state.conn)
            .await?;

            UserTokenUsageStats::find()
                .filter(user_token_usage_stats::Column::Id.eq(insertion.last_insert_id))
                .one(&self.server_state.conn)
                .await?
                .context("Could not find inserted token usage record")?
        };

        self.set_token_count(inserted.tokens_consumed);

        Ok(())
    }

    #[allow(dead_code)]
    pub fn cancel(&self) {
        self.cancelled.store(true, Relaxed);
    }

    fn set_token_count(&self, tokens: i64) {
        self.token_count.store(tokens, Relaxed);
    }

    fn fetch_add_token_count(&self, tokens: i64) -> i64 {
        self.token_count.fetch_add(tokens, Relaxed)
    }

    pub fn is_quota_reached(&self) -> bool {
        self.token_count.load(Relaxed) >= self.remaining_quota
    }

    pub fn current_token_usage(&self) -> i64 {
        self.token_count.load(Relaxed)
    }

    pub fn total_emails_processed(&self) -> i64 {
        self.processed_email_count.load(Relaxed)
    }

    pub fn fetch_add_total_emails_processed(&self, count: i64) -> i64 {
        self.processed_email_count.fetch_add(count, Relaxed)
    }

    pub fn total_emails_failed(&self) -> i64 {
        self.failed_email_count.load(Relaxed)
    }

    pub fn fetch_add_total_emails_failed(&self, count: i64) -> i64 {
        self.failed_email_count.fetch_add(count, Relaxed)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Relaxed)
    }

    pub fn emails_remaining(&self) -> i64 {
        self.email_queue.len() as i64
    }

    pub fn status(&self) -> EmailProcessorStatusUpdate {
        let status = if self.check_is_processing() {
            ProcessorStatus::Processing
        } else if self.is_cancelled() {
            ProcessorStatus::Cancelled
        } else if self.is_quota_reached() {
            ProcessorStatus::QuotaExceeded
        } else {
            ProcessorStatus::Completed
        };

        EmailProcessorStatusUpdate {
            status,
            emails_processed: self.total_emails_processed(),
            emails_failed: self.total_emails_failed(),
            emails_remaining: self.emails_remaining(),
        }
    }
}

// Helper functions
fn parse_id_to_int(id: String) -> u128 {
    u128::from_str_radix(&id, 16).expect("Could not parse email id to integer")
}

fn parse_int_to_id(id: u128) -> String {
    format!("{:x}", id)
}
