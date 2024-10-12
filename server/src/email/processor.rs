use std::{
    collections::HashMap,
    sync::{atomic::AtomicI64, Arc},
};

use anyhow::Context;
use entity::{email_training, prelude::*, processed_email, user_session, user_token_usage_stats};
use futures::{future::join_all, FutureExt};
use sea_orm::{
    entity::*, prelude::Expr, query::*, sea_query::OnConflict, ActiveValue, EntityTrait,
    FromQueryResult,
};
use std::sync::atomic::Ordering::Relaxed;

use crate::{
    email::client::EmailClient,
    server_config::{cfg, UNKNOWN_CATEGORY},
    structs::error::{AppError, AppResult},
    ServerState,
};
use crate::{
    email::client::MessageListOptions,
    prompt::mistral::{self, CategoryPromptResponse},
};

use super::client::EmailMessage;

// This struct processes emails for a single user
pub struct EmailProcessor {
    pub user_session_id: i32,
    pub email: String,
    email_client: EmailClient,
    server_state: ServerState,
    token_count: Arc<AtomicI64>,
    pub processed_email_count: Arc<AtomicI64>,
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

        // -- Debug
        println!("User's current usage: {}", quota_used);
        println!("User's remaining quota: {}", remaining_quota);
        // -- Debug

        Ok(EmailProcessor {
            user_session_id: user_session.id,
            email: user_session.email,
            server_state,
            email_client,
            token_count: Arc::new(AtomicI64::new(quota_used)),
            processed_email_count: Arc::new(AtomicI64::new(0)),
        })
    }

    pub async fn process_email(
        &self,
        email_message: &EmailMessage,
    ) -> AppResult<EmailProcessedStatus> {
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
                user_email: ActiveValue::Set(self.email.clone()),
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
            user_session_email: ActiveValue::Set(self.email.clone()),
            labels_applied: ActiveValue::Set(label_update.added),
            labels_removed: ActiveValue::Set(label_update.removed),
            ai_answer: ActiveValue::Set(category_content),
            processed_at: ActiveValue::NotSet,
        })
        .exec(&self.server_state.conn)
        .await?;

        Ok::<_, AppError>(EmailProcessedStatus::Success(token_usage))
    }

    pub async fn process_full_sync(&self) -> anyhow::Result<()> {
        tracing::info!("Processing emails for {}", self.email);
        match self.email_client.configure_labels_if_needed().await {
            Ok(true) => {
                tracing::info!("Labels configured successfully for {}", self.email);
            }
            Ok(false) => {
                tracing::info!("Labels already configured for {}", self.email);
            }
            Err(e) => {
                tracing::error!("Error configuring labels for {}: {:?}", self.email, e);
                return Err(e);
            }
        }

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
        struct PartialProcessedEmail {
            id: String,
            ai_answer: String,
        }

        let already_processed_ids = Arc::new(
            processed_email::Entity::find()
                .filter(processed_email::Column::Id.is_in(message_ids))
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

        let mut tasks = message_list_resp
            .messages
            .context("No messages found")?
            .into_iter()
            .map(|message| {
                let server_state = self.server_state.clone();
                let already_processed_ids = already_processed_ids.clone();
                async move {
                    let email_id = message.id.context("Email did not have an id")?.clone();
                    if let Some(processed) = already_processed_ids.get(&email_id) {
                        tracing::info!(
                            "Email {} already processed into {}, skipping",
                            processed.id,
                            processed.ai_answer
                        );
                        return Ok::<_, AppError>(EmailProcessedStatus::Skipped);
                    }
                    // -- Debug
                    // println!("Processing email: {:?}", email_message);
                    // -- Debug

                    let email_message = self.email_client.get_sanitized_message(&email_id).await?;
                    // Acquire rate limiter tokens
                    server_state.rate_limiters.acquire_one().await;
                    match self.process_email(&email_message).await {
                        Ok(status) => Ok::<_, AppError>(status),
                        Err(e) => {
                            tracing::error!("Error processing email {}: {:?}", email_id, e);
                            Err::<_, AppError>(e)
                        }
                    }
                }
                .boxed()
            })
            .collect::<Vec<_>>();

        let mut emails_processed = 0;
        let mut emails_skipped = 0;
        let mut emails_failed = 0;

        const CHUNK_SIZE: usize = 5;
        for chunk in tasks.chunks_mut(CHUNK_SIZE) {
            if self.is_quota_reached() {
                println!("Quota reached for {}, stopping processing...", self.email);
                break;
            }

            let mut total_tokens_used = 0;
            for result in join_all(chunk).await {
                match result {
                    Ok(EmailProcessedStatus::Success(token_usage)) => {
                        emails_processed += 1;
                        total_tokens_used += token_usage;
                    }
                    Ok(EmailProcessedStatus::Skipped) => {
                        emails_skipped += 1;
                    }
                    Err(e) => {
                        tracing::error!("Error processing email: {:?}", e);
                        emails_failed += 1;
                    }
                }
            }

            match self.add_tally_to_user_daily_quota(total_tokens_used).await {
                Ok(_) => {}
                Err(e) => tracing::error!("Error adding tally to user daily quota: {:?}", e),
            }

            println!(
                "{} total emails processed for {}",
                emails_processed, self.email
            );
            println!("{} total emails skipped for {}", emails_skipped, self.email);
            println!("{} total emails failed for {}", emails_failed, self.email);
            println!(
                "Current token usage: {}, Remaining quota: {}",
                self.current_token_usage(),
                self.remaining_quota()
            );
        }

        tracing::info!(
            "Email processing complete for {}, {} tokens used, {} emails processed",
            self.email,
            self.current_token_usage(),
            self.total_emails_processed()
        );

        Ok(())
    }

    async fn add_tally_to_user_daily_quota(&self, tokens: i64) -> anyhow::Result<()> {
        tracing::info!(
            "Adding {} tokens to user {}'s daily quota",
            tokens,
            self.email
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

    fn set_token_count(&self, tokens: i64) {
        self.token_count.store(tokens, Relaxed);
    }

    fn is_quota_reached(&self) -> bool {
        self.token_count.load(Relaxed) >= cfg.api.token_limits.daily_user_quota as i64
    }

    fn current_token_usage(&self) -> i64 {
        self.token_count.load(Relaxed)
    }

    fn remaining_quota(&self) -> i64 {
        cfg.api.token_limits.daily_user_quota as i64 - self.token_count.load(Relaxed)
    }

    fn total_emails_processed(&self) -> i64 {
        self.processed_email_count.load(Relaxed)
    }
}

pub enum EmailProcessedStatus {
    Success(i64),
    Skipped,
}
