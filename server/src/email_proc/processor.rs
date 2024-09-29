use std::sync::{atomic::AtomicU64, Arc};

use arl::RateLimiter;
use entity::user_session;
use std::sync::atomic::Ordering::Relaxed;

use crate::{
    email_client::EmailClient, email_proc::prompt, routes::auth, structs::error::AppResult,
    ServerState,
};

pub struct EmailProcessor {
    pub email: String,
    email_client: EmailClient,
    pub server_state: ServerState,
    pub token_count: Arc<AtomicU64>,
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
            resp.access_token
        } else {
            user_session.access_token
        };

        let email_client = EmailClient::new(http_client.clone(), access_token).await?;

        tracing::info!(
            "Email client created successfully for {}",
            user_session.email
        );

        Ok(EmailProcessor {
            server_state,
            token_count: Arc::new(AtomicU64::new(0)),
            email: user_session.email,
            email_client,
        })
    }

    pub fn add_token_count(&self, count: u64) {
        self.token_count.fetch_add(count, Relaxed);
    }

    pub async fn process(&self) -> anyhow::Result<()> {
        tracing::info!("Processing emails for {}", self.email);

        let email_messages = self.email_client.get_messages().await?;

        for email_message in email_messages {
            prompt::send_category_prompt(
                self.server_state.clone(),
                self.token_count.clone(),
                email_message,
            );
        }

        tracing::info!(
            "Email processing complete for {}, {} tokens used",
            self.email,
            self.token_count.load(Relaxed)
        );

        Ok(())
    }
}
