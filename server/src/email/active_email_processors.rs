use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, Context};
use indexmap::IndexSet;
use tokio::{sync::RwLock, task::JoinHandle};

use crate::ServerState;

use crate::email::processor::EmailProcessor;

struct ActiveEmailProcessors {
    server_state: ServerState,
    waiting_queue: IndexSet<String>,
    email_to_active_proc: HashMap<String, Arc<EmailProcessor>>,
    max_size: usize,
}

impl ActiveEmailProcessors {
    pub fn new(server_state: ServerState, max_size: usize) -> Self {
        Self {
            server_state,
            waiting_queue: IndexSet::new(),
            email_to_active_proc: HashMap::new(),
            max_size,
        }
    }

    // Inserting a new processor into the queue will start processing emails for that user
    // If the queue is full, the user will be added to the waiting queue
    // If the user is already being processed, new emails will be fetched and added to the emails being processed
    pub async fn insert_processor(&mut self, email_address: String) -> anyhow::Result<()> {
        if let Some(proc) = self.email_to_active_proc.get(&email_address) {
            let proc = proc.clone();
            tokio::spawn(async move {
                proc.queue_new_emails().await.unwrap_or_else(|e| {
                    tracing::error!("Error queueing new emails: {:?}", e);
                });
            });

            return Ok(());
        }
        if self.email_to_active_proc.len() >= self.max_size {
            // Queue is full, add to waiting queue
            self.waiting_queue.insert(email_address.clone());
            return Ok(());
        }

        let proc = Arc::new(
            EmailProcessor::from_email_address(self.server_state.clone(), &email_address)
                .await
                .map_err(|e| anyhow!("Could not create email processor {:?}", e))?,
        );
        {
            let proc = proc.clone();
            tokio::spawn(async move {
                proc.queue_new_emails().await.unwrap_or_else(|e| {
                    tracing::error!("Error queueing new emails: {:?}", e);
                });
                proc.process_emails_in_queue()
                    .await
                    .unwrap_or_else(|e| tracing::error!("Error processing emails: {:?}", e));
            });
        }
        self.email_to_active_proc
            .insert(email_address.clone(), proc);

        Ok(())
    }

    pub async fn remove_processor(&mut self, email_address: &str) -> anyhow::Result<()> {
        self.email_to_active_proc
            .remove(email_address)
            .context("No processor was removed")?;
        if let Some(next_in_line) = self.waiting_queue.swap_remove_index(0) {
            self.insert_processor(next_in_line)
                .await
                .context("Next in waiting list was not added")?;
        }

        Ok(())
    }

    // pub fn get_processor_by_email(
    //     &self,
    //     email_address: &str,
    // ) -> Option<Arc<RwLock<EmailProcessor>>> {
    //     self.email_to_active_proc.get(email_address).cloned()
    // }

    // pub fn get_all_processors(&self) -> Vec<Arc<RwLock<EmailProcessor>>> {
    //     self.email_to_active_proc
    //         .values()
    //         .map(|proc| proc.clone())
    //         .collect()
    // }

    pub async fn get_queue_status(&self) {
        let mut display_str = "Email Processing Queue Status:\n".to_string();
        if self.email_to_active_proc.is_empty() {
            display_str.push_str("No active processors\n");
        } else {
            display_str.push_str(&format!(
                "Active Processors:{}\n",
                self.email_to_active_proc.len()
            ));
        }
        if self.waiting_queue.is_empty() {
            display_str.push_str("No users waiting\n");
        } else {
            display_str.push_str(&format!("Users waiting:{}\n", self.waiting_queue.len()));
        }

        for (email, proc) in self.email_to_active_proc.iter() {
            let status = proc.status();
            display_str.push_str(&format!("\t{} -> {}\n", email, status));
        }
        tracing::info!("{}", display_str);
    }

    pub async fn cleanup_finished_processors(&mut self) {
        let mut to_remove = vec![];
        for (email, proc) in self.email_to_active_proc.iter() {
            if proc.check_is_processing() {
                continue;
            }
            to_remove.push(email.clone());
        }

        for email in to_remove {
            self.remove_processor(&email).await.unwrap_or_else(|e| {
                tracing::error!("Error removing processor: {:?}", e);
            });
        }
    }
}

#[derive(Clone)]
pub struct ActiveProcessingQueue {
    active_processors: Arc<RwLock<ActiveEmailProcessors>>,
}

impl ActiveProcessingQueue {
    pub fn new(server_state: ServerState, max_size: usize) -> Self {
        Self {
            active_processors: Arc::new(RwLock::new(ActiveEmailProcessors::new(
                server_state,
                max_size,
            ))),
        }
    }

    pub async fn has_email(&self, email_address: &str) -> bool {
        let queue = self.active_processors.read().await;
        queue.email_to_active_proc.contains_key(email_address)
    }

    pub async fn add_to_processing(&self, email_address: String) -> anyhow::Result<()> {
        let mut queue = self.active_processors.write().await;
        queue.insert_processor(email_address.to_string()).await
    }

    // pub async fn get_processor_status_by_email(&self, email_address: &str) -> bool {
    //     let queue = self.active_processors.lock().await;
    //     if let Some(proc) = queue.get_processor_by_email(email_address) {
    //         let proc = proc.read().await;
    //         proc.check_is_processing()
    //     } else {
    //         false
    //     }
    // }

    // pub async fn is_empty(&self) -> bool {
    //     let queue = self.active_processors.read().await;
    //     queue.email_to_active_proc.is_empty()
    // }

    pub async fn cleanup_finished_processors(&self) {
        let mut queue = self.active_processors.write().await;
        queue.cleanup_finished_processors().await;
    }

    pub async fn watch(&self) -> JoinHandle<()> {
        let queue = self.active_processors.clone();
        async fn print_status_map(queue: Arc<RwLock<ActiveEmailProcessors>>) {
            let queue = queue.read().await;
            queue.get_queue_status().await;
        }

        print_status_map(queue.clone()).await;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                print_status_map(queue.clone()).await;
            }
        })
    }
}
