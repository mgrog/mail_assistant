use std::sync::RwLock;
use std::time::Duration;
use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, Context};
use tokio::task::JoinHandle;
use tokio::time::interval;

use crate::model::user::UserWithAccountAccessAndUsage;
use crate::ServerState;

use crate::email::processor::EmailProcessor;

use super::processor::ProcessorStatus;

type EmailProcessorMap = HashMap<String, Arc<EmailProcessor>>;

#[derive(Clone)]
pub struct ActiveEmailProcessorMap {
    server_state: ServerState,
    active_processors: Arc<RwLock<EmailProcessorMap>>,
}

impl ActiveEmailProcessorMap {
    pub fn new(server_state: ServerState) -> Self {
        Self {
            server_state,
            active_processors: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn insert_processor(
        &self,
        user: UserWithAccountAccessAndUsage,
    ) -> anyhow::Result<Arc<EmailProcessor>> {
        let user_email = user.email.clone();

        if let Some(processor) = self.active_processors.read().unwrap().get(&user_email) {
            return Ok(processor.clone());
        }

        let proc = Arc::new(
            EmailProcessor::new(self.server_state.clone(), user)
                .await
                .map_err(|e| anyhow!("Could not create email processor {:?}", e))?,
        );

        {
            let proc = proc.clone();
            let user_email = user_email.clone();
            tokio::spawn(async move {
                match proc.run().await {
                    Ok(_) => {
                        tracing::info!("Processor for {} finished", user_email);
                    }
                    Err(e) => {
                        tracing::error!("Processor for {} failed: {:?}", user_email, e);
                    }
                };
            });
        }

        let result = self
            .active_processors
            .write()
            .unwrap()
            .insert(user_email, proc)
            .context("Could not insert processor");

        self.get_current_state();

        result
    }

    pub fn get_current_state(&self) -> Option<String> {
        let active_processors = self.active_processors.read().unwrap();
        if active_processors.is_empty() {
            return None;
        }

        let mut display_str = "Email Processing Queue Status:\n".to_string();
        display_str.push_str(&format!("Active Processors:{}\n", active_processors.len()));

        for (email, proc) in active_processors.iter() {
            let status = proc.get_current_state();
            display_str.push_str(&format!("\t{} -> {:?}\n", email, status));
        }

        Some(display_str)
    }

    pub fn cleanup_stopped_processors(&self) {
        self.active_processors.write().unwrap().retain(|_, proc| {
            !matches!(
                proc.status(),
                ProcessorStatus::Cancelled
                    | ProcessorStatus::Failed
                    | ProcessorStatus::QuotaExceeded
            )
        });

        self.get_current_state();
    }

    pub fn cancel_processor(&self, email_address: &str) {
        if let Some(processor) = self.active_processors.read().unwrap().get(email_address) {
            tracing::info!("Cancelling processor for {}", email_address);
            processor.cancel();
        } else {
            tracing::info!("No active processor found for {}", email_address);
        }
    }

    pub fn get(&self, email_address: &str) -> Option<Arc<EmailProcessor>> {
        self.active_processors
            .read()
            .unwrap()
            .get(email_address)
            .cloned()
    }

    pub fn total_emails_processed(&self) -> i64 {
        self.active_processors
            .read()
            .unwrap()
            .values()
            .map(|p| p.total_emails_processed())
            .sum()
    }

    pub fn watch(&self) -> JoinHandle<()> {
        let mut interval = interval(Duration::from_secs(5));
        let mut now = std::time::Instant::now();
        let mut last_recorded = 0;
        let self_ = self.clone();
        tokio::spawn(async move {
            loop {
                interval.tick().await;
                let diff = self_.total_emails_processed() - last_recorded;
                let emails_per_second = diff as f64 / now.elapsed().as_secs_f64();
                now = std::time::Instant::now();
                last_recorded = self_.total_emails_processed();
                if let Some(update) = self_.get_current_state() {
                    tracing::info!(
                        "Processor Status Update:\n{email_per_second:.2} emails/s\n{update}",
                        email_per_second = emails_per_second,
                        update = update
                    );
                }
            }
        })
    }

    pub fn len(&self) -> usize {
        self.active_processors.read().unwrap().len()
    }
}
