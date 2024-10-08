use std::{
    collections::VecDeque,
    sync::{atomic::AtomicI64, Arc},
};

use anyhow::Context;
use chrono::DateTime;
use entity::{prelude::*, processed_email, user_session, user_token_usage_stats};
use futures::future::join_all;
use sea_orm::{entity::*, prelude::Expr, query::*, ActiveValue, EntityTrait};
use std::sync::atomic::Ordering::Relaxed;

use crate::{
    email::client::EmailClient,
    prompt::{self, CategoryPromptResponse},
    routes::auth,
    server_config::{cfg, UNKNOWN_CATEGORY},
    structs::error::{AppError, AppResult},
    ServerState,
};

// This struct processes emails for a single user
pub struct EmailProcessor {
    pub user_session_id: i32,
    pub email: String,
    email_client: EmailClient,
    server_state: ServerState,
    token_count: Arc<AtomicI64>,
    remaining_quota: i64,
    pub processed_email_count: Arc<AtomicI64>,
}

impl EmailProcessor {
    pub async fn new(
        server_state: ServerState,
        user_session: user_session::Model,
    ) -> AppResult<Self> {
        let http_client = server_state.http_client.clone();
        let access_token = if user_session.expires_at < chrono::Utc::now() {
            let resp =
                auth::exchange_refresh_token(http_client.clone(), user_session.refresh_token)
                    .await?;
            user_session::Entity::update(user_session::ActiveModel {
                id: ActiveValue::Set(user_session.id),
                access_token: ActiveValue::Set(resp.access_token.clone()),
                expires_at: ActiveValue::Set(DateTime::from(
                    chrono::Utc::now() + chrono::Duration::seconds(resp.expires_in as i64),
                )),
                ..Default::default()
            })
            .exec(&server_state.conn)
            .await?;
            resp.access_token
        } else {
            user_session.access_token
        };

        let email_client = EmailClient::new(http_client.clone(), access_token).await?;

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

        let remaining_quota = cfg.daily_user_quota as i64 - quota_used;

        // -- Debug
        println!(
            "Quota used: {}, Remaining quota: {}",
            quota_used, remaining_quota
        );
        // -- Debug

        Ok(EmailProcessor {
            user_session_id: user_session.id,
            email: user_session.email,
            server_state,
            email_client,
            token_count: Arc::new(AtomicI64::new(0)),
            remaining_quota,
            processed_email_count: Arc::new(AtomicI64::new(0)),
        })
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

        // First page of emails
        let message_list_resp = self.email_client.get_message_list(None).await?;

        let mut tasks = message_list_resp
            .messages
            .context("No messages found")?
            .into_iter()
            .take(1)
            .map(|email_message| {
                let server_state = self.server_state.clone();
                let user_token_count = self.token_count.clone();
                async move {
                    if self.is_quota_reached() {
                        return Ok::<_, AppError>(());
                    }

                    let email_id = email_message
                        .id
                        .context("Email did not have an id")?
                        .clone();
                    if let Some(processed) = processed_email::Entity::find_by_id(&email_id)
                        .one(&server_state.conn)
                        .await?
                    {
                        tracing::info!(
                            "Email {} already processed into {}, skipping",
                            processed.id,
                            processed.ai_answer
                        );
                        return Ok::<_, AppError>(());
                    }
                    let email_message = self.email_client.get_sanitized_message(&email_id).await?;
                    let current_labels = email_message.label_ids.clone();

                    // -- Debug
                    // println!("Processing email: {:?}", email_message);
                    // -- Debug

                    // Acquire rate limiter tokens
                    server_state
                        .rate_limiters
                        .acquire(cfg.estimated_token_usage_per_email as usize)
                        .await;

                    let CategoryPromptResponse {
                        category,
                        token_usage,
                    } = match prompt::send_category_prompt_rate_limited(
                        &server_state,
                        email_message,
                    )
                    .await
                    {
                        Ok(resp) => Ok::<_, AppError>(resp),
                        Err(e) => {
                            tracing::error!("Error processing email {}: {:?}", email_id, e);
                            return Err::<_, AppError>(e);
                        }
                    }?;

                    // Add to specific user token count for quota
                    user_token_count.fetch_add(token_usage, Relaxed);
                    // Add to total token count
                    server_state.add_global_token_count(token_usage);

                    // let label_category = cfg
                    //     .categories
                    //     .iter()
                    //     .find(|c| c.ai == category)
                    //     .unwrap_or(&UNKNOWN_CATEGORY);

                    // let label_update = self
                    //     .email_client
                    //     .label_email(email_id.clone(), current_labels, label_category.clone())
                    //     .await?;

                    // ProcessedEmail::insert(processed_email::ActiveModel {
                    //     id: ActiveValue::Set(email_id),
                    //     user_session_id: ActiveValue::Set(self.user_session_id),
                    //     user_session_email: ActiveValue::Set(self.email.clone()),
                    //     labels_applied: ActiveValue::Set(label_update.added),
                    //     labels_removed: ActiveValue::Set(label_update.removed),
                    //     ai_answer: ActiveValue::Set(category),
                    //     processed_at: ActiveValue::NotSet,
                    // })
                    // .exec(&server_state.conn)
                    // .await?;

                    self.processed_email_count.fetch_add(1, Relaxed);

                    Ok::<_, AppError>(())
                }
            })
            .collect::<VecDeque<_>>();

        const CHUNK_SIZE: usize = 5;
        while !tasks.is_empty() {
            if self.is_quota_reached() {
                println!("Quota reached for {}, stopping processing...", self.email);
                break;
            }
            let mut chunk = vec![];
            for _ in 0..CHUNK_SIZE {
                if let Some(task) = tasks.pop_front() {
                    chunk.push(task);
                }
            }
            println!("Processing chunk of {} emails", chunk.len());
            join_all(chunk).await;
        }

        // self.add_tally_to_user_daily_quota().await?;

        tracing::info!(
            "Email processing complete for {}, {} tokens used, {} emails processed",
            self.email,
            self.current_token_usage(),
            self.total_emails_processed()
        );

        Ok(())
    }

    async fn add_tally_to_user_daily_quota(&self) -> anyhow::Result<()> {
        let current_user_token_count = self.token_count.load(Relaxed);
        tracing::info!(
            "Adding {} tokens to user {}'s daily quota",
            current_user_token_count,
            self.email
        );

        // Update the user's token usage in the database
        let existing = UserTokenUsageStats::find()
            .filter(user_token_usage_stats::Column::UserSessionId.eq(self.user_session_id))
            .filter(user_token_usage_stats::Column::Date.eq(chrono::Utc::now().date_naive()))
            .one(&self.server_state.conn)
            .await?;

        if let Some(existing) = existing {
            UserTokenUsageStats::update_many()
                .filter(user_token_usage_stats::Column::Id.eq(existing.id))
                .col_expr(
                    user_token_usage_stats::Column::TokensConsumed,
                    Expr::col(user_token_usage_stats::Column::TokensConsumed)
                        .add(current_user_token_count),
                )
                .to_owned()
                .exec(&self.server_state.conn)
                .await?;
        } else {
            UserTokenUsageStats::insert(user_token_usage_stats::ActiveModel {
                id: ActiveValue::NotSet,
                user_session_id: ActiveValue::Set(self.user_session_id),
                tokens_consumed: ActiveValue::Set(current_user_token_count),
                date: ActiveValue::NotSet,
                created_at: ActiveValue::NotSet,
                updated_at: ActiveValue::NotSet,
            })
            .exec(&self.server_state.conn)
            .await?;
        }

        Ok(())
    }

    fn is_quota_reached(&self) -> bool {
        self.token_count.load(Relaxed) >= self.remaining_quota
    }

    fn current_token_usage(&self) -> i64 {
        self.token_count.load(Relaxed)
    }

    fn total_emails_processed(&self) -> i64 {
        self.processed_email_count.load(Relaxed)
    }
}
