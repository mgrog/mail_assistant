use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use chrono::Utc;

#[derive(Debug, Copy, Clone)]
pub struct AuthSession {
    pub expires_at: i64,
}

#[derive(Debug, Clone)]
pub struct AuthSessionStore {
    inner: Arc<RwLock<HashMap<String, AuthSession>>>,
}

impl AuthSessionStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn load_session(&self, session_id: &str) -> Option<AuthSession> {
        self.inner.read().unwrap().get(session_id).copied()
    }

    pub fn store_session(&self, session_id: String) {
        let session = AuthSession {
            expires_at: Utc::now().timestamp() + 120,
        };
        self.inner.write().unwrap().insert(session_id, session);
    }

    pub fn destroy_session(&self, session_id: &str) {
        self.inner.write().unwrap().remove(session_id);
    }

    pub fn clean_store(&self) {
        let now = Utc::now().timestamp();
        self.inner
            .write()
            .unwrap()
            .retain(|_, session| session.expires_at > now);
    }
}
