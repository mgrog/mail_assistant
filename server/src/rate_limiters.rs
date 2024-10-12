use std::{sync::Arc, time::Duration};

use leaky_bucket::RateLimiter;

use crate::server_config::cfg;

#[derive(Clone)]
pub struct RateLimiters {
    prompt: Arc<RateLimiter>,
    token: Arc<RateLimiter>,
}

impl RateLimiters {
    pub fn from_env() -> Self {
        let prompt = RateLimiter::builder()
            .initial(cfg.api.prompt_limits.rate_limit_per_sec / 2)
            .interval(Duration::from_millis(
                cfg.api.prompt_limits.refill_interval_ms as u64,
            ))
            .max(cfg.api.prompt_limits.rate_limit_per_sec)
            .refill(cfg.api.prompt_limits.refill_amount)
            .build();
        let token = RateLimiter::builder()
            .initial(cfg.api.token_limits.rate_limit_per_min)
            .interval(Duration::from_millis(
                cfg.api.token_limits.refill_interval_ms as u64,
            ))
            .refill(cfg.api.token_limits.refill_amount)
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

    pub async fn acquire_one(&self) {
        self.prompt.acquire_one().await;
    }
}
