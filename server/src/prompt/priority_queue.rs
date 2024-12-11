use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap, HashSet},
    sync::{Arc, Mutex, RwLock},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    High,
    Low,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PromptQueueEmailEntry {
    pub user_email: String,
    pub email_id: u128,
    pub priority: Priority,
}

impl Ord for PromptQueueEmailEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        use Priority::*;
        match (self, other) {
            (Self { priority: High, .. }, Self { priority: Low, .. }) => Ordering::Greater,
            (Self { priority: Low, .. }, Self { priority: High, .. }) => Ordering::Less,
            _ => Ordering::Equal,
        }
    }
}

impl PartialOrd for PromptQueueEmailEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct QueueCount {
    pub high_priority: usize,
    pub low_priority: usize,
}

#[derive(Debug, Clone)]
pub struct PromptPriorityQueue {
    queue: Arc<Mutex<BinaryHeap<PromptQueueEmailEntry>>>,
    num_in_queue_by_email_address: Arc<RwLock<HashMap<String, QueueCount>>>,
    in_processing_set: Arc<Mutex<HashSet<u128>>>,
}

impl PromptPriorityQueue {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(BinaryHeap::new())),
            num_in_queue_by_email_address: Arc::new(RwLock::new(HashMap::new())),
            in_processing_set: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn push(&self, user_email: String, email_id: u128, priority: Priority) -> bool {
        let mut queue = self.queue.lock().unwrap();
        let mut num_in_queue_by_email_address = self.num_in_queue_by_email_address.write().unwrap();
        let mut in_processing_set = self.in_processing_set.lock().unwrap();

        if !in_processing_set.insert(email_id) {
            return false;
        }

        queue.push(PromptQueueEmailEntry {
            user_email: user_email.clone(),
            email_id,
            priority,
        });

        num_in_queue_by_email_address
            .entry(user_email)
            .and_modify(|e| {
                if priority == Priority::High {
                    e.high_priority += 1;
                } else {
                    e.low_priority += 1;
                }
            })
            .or_insert(if priority == Priority::High {
                QueueCount {
                    high_priority: 1,
                    low_priority: 0,
                }
            } else {
                QueueCount {
                    high_priority: 0,
                    low_priority: 1,
                }
            });

        true
    }

    pub fn pop(&self) -> Option<PromptQueueEmailEntry> {
        let mut queue = self.queue.lock().unwrap();
        let next = queue.pop();
        if let Some(entry) = next {
            let mut num_in_queue_by_email_address =
                self.num_in_queue_by_email_address.write().unwrap();

            if let Some(count) = num_in_queue_by_email_address.get_mut(&entry.user_email) {
                match (count.high_priority, count.low_priority) {
                    (0, 0) => {
                        num_in_queue_by_email_address.remove(&entry.user_email);
                    }
                    _ => {
                        if entry.priority == Priority::High {
                            count.high_priority -= 1;
                        } else {
                            count.low_priority -= 1;
                        }
                    }
                }
            }

            Some(entry)
        } else {
            next
        }
    }

    pub fn remove_from_processing(&self, email_id: u128) {
        let mut in_processing_set = self.in_processing_set.lock().unwrap();
        in_processing_set.remove(&email_id);
    }

    pub fn num_in_queue(&self, email_address: &str) -> usize {
        self.num_in_queue_by_email_address
            .read()
            .unwrap()
            .get(email_address)
            .map(|e| e.high_priority + e.low_priority)
            .unwrap_or(0)
    }

    pub fn num_high_priority_in_queue(&self, email_address: &str) -> usize {
        self.num_in_queue_by_email_address
            .read()
            .unwrap()
            .get(email_address)
            .map(|e| e.high_priority)
            .unwrap_or(0)
    }

    pub fn num_low_priority_in_queue(&self, email_address: &str) -> usize {
        self.num_in_queue_by_email_address
            .read()
            .unwrap()
            .get(email_address)
            .map(|e| e.low_priority)
            .unwrap_or(0)
    }

    pub fn all_high_priority(&self) -> usize {
        self.num_in_queue_by_email_address
            .read()
            .unwrap()
            .values()
            .map(|e| e.high_priority)
            .sum()
    }

    pub fn len(&self) -> usize {
        self.num_in_queue_by_email_address
            .read()
            .unwrap()
            .values()
            .map(|e| e.high_priority + e.low_priority)
            .sum()
    }

    pub fn num_in_processing(&self) -> usize {
        self.in_processing_set.lock().unwrap().len()
    }
}
