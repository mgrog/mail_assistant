use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use indexmap::IndexSet;

use super::parse_id_to_int;

#[derive(Debug, Clone)]
pub struct EmailProcessingQueue {
    queue: Arc<Mutex<IndexSet<u128>>>,
    currently_processing: Arc<Mutex<HashSet<u128>>>,
}

impl EmailProcessingQueue {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(IndexSet::new())),
            currently_processing: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn add_to_queue(&self, email_ids: Vec<u128>) -> i32 {
        let mut q = self.queue.lock().unwrap();
        let p = self.currently_processing.lock().unwrap();
        let mut num_added = 0;
        for email_id in email_ids {
            if p.contains(&email_id) {
                continue;
            }
            if q.insert(email_id) {
                num_added += 1;
            }
        }
        num_added
    }

    pub fn add_to_currently_processing(&self, email_id: u128) {
        self.currently_processing.lock().unwrap().insert(email_id);
    }

    pub fn remove_from_currently_processing(&self, email_id: u128) {
        self.currently_processing.lock().unwrap().remove(&email_id);
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

    pub fn num_in_processing(&self) -> i64 {
        self.currently_processing.lock().unwrap().len() as i64
    }
}

pub enum EmailQueueStatus {
    Cancelled,
    QuotaExceeded,
    Complete,
}
