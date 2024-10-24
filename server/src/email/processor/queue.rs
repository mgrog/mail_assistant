use std::sync::{Arc, Mutex};

use indexmap::IndexSet;

#[derive(Debug, Clone)]
pub struct EmailProcessingQueue {
    queue: Arc<Mutex<IndexSet<u128>>>,
}

impl EmailProcessingQueue {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(IndexSet::new())),
        }
    }

    pub fn add_to_queue(&self, email_id: u128) {
        self.queue.lock().unwrap().insert(email_id);
    }

    pub fn get_next(&self) -> Option<u128> {
        self.queue.lock().unwrap().swap_remove_index(0)
    }

    pub fn is_empty(&self) -> bool {
        self.queue.lock().unwrap().is_empty()
    }

    pub fn len(&self) -> usize {
        self.queue.lock().unwrap().len()
    }
}

pub enum EmailQueueStatus {
    Cancelled,
    QuotaExceeded,
    Complete,
}
