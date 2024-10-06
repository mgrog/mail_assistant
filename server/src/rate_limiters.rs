use std::{sync::Arc, time::Duration};

use leaky_bucket::RateLimiter;

use crate::server_config::CONFIG;

#[derive(Clone)]
pub struct RateLimiters {
    prompt: Arc<RateLimiter>,
    token: Arc<RateLimiter>,
}

impl RateLimiters {
    pub fn from_env() -> Self {
        let duration_1_min = Duration::from_secs(60);
        let prompt = RateLimiter::builder()
            .initial(CONFIG.prompt_rate_limit_per_min as usize)
            .interval(duration_1_min)
            .refill(CONFIG.prompt_rate_limit_per_min as usize)
            .build();
        let token = RateLimiter::builder()
            .initial(CONFIG.token_rate_limit_per_min as usize)
            .interval(duration_1_min)
            .refill(CONFIG.token_rate_limit_per_min as usize)
            .build();
        Self {
            prompt: Arc::new(prompt),
            token: Arc::new(token),
        }
    }
    pub async fn acquire(&self, token_count: usize) {
        self.prompt.acquire_one().await;
        self.token.acquire(token_count).await;
    }
}
