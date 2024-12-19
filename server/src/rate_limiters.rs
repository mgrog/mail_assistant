use std::sync::atomic::Ordering::Relaxed;
use std::sync::{atomic::AtomicBool, Arc};
use tokio::time::Duration;

use leaky_bucket::RateLimiter;

use crate::server_config::cfg;

#[derive(Clone)]
pub struct RateLimiters {
    prompt: Arc<RateLimiter>,
    backoff: Arc<AtomicBool>,
    backoff_duration: Duration,
}

impl RateLimiters {
    pub fn new(prompt_limit_per_sec: usize, interval_ms: usize, refill: usize) -> Self {
        let prompt = RateLimiter::builder()
            .initial(prompt_limit_per_sec)
            .interval(Duration::from_millis(interval_ms as u64))
            .max(prompt_limit_per_sec)
            .refill(refill)
            .build();

        Self {
            prompt: Arc::new(prompt),
            backoff: Arc::new(AtomicBool::new(false)),
            backoff_duration: Duration::from_secs(60),
        }
    }

    pub fn from_env() -> Self {
        let prompt_limit_per_sec = cfg.api.prompt_limits.rate_limit_per_sec;
        let interval_ms = cfg.api.prompt_limits.refill_interval_ms;
        let refill = cfg.api.prompt_limits.refill_amount;
        Self::new(prompt_limit_per_sec, interval_ms, refill)
    }

    pub async fn acquire_one(&self) {
        if self.backoff.load(Relaxed) {
            tokio::time::sleep(self.backoff_duration).await;
        }
        self.prompt.acquire_one().await;
    }

    pub fn trigger_backoff(&self) {
        tracing::info!("Triggering backoff...");
        self.backoff
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let self_ = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(60)).await;
            tracing::info!("Backoff expired");
            self_
                .backoff
                .store(false, std::sync::atomic::Ordering::Relaxed);
        });
    }

    pub fn get_status(&self) -> String {
        format!("{}/{}", self.prompt.balance(), self.prompt.max(),)
    }
}
