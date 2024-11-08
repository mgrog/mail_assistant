use crate::envs::{self, get_env_b64u_as_u8s};
use std::sync::OnceLock;

pub fn auth_config() -> &'static AuthConfig {
    static INSTANCE: OnceLock<AuthConfig> = OnceLock::new();

    INSTANCE.get_or_init(|| {
        AuthConfig::load_from_env()
            .unwrap_or_else(|ex| panic!("FATAL - WHILE LOADING CONF - Cause: {ex:?}"))
    })
}

#[allow(non_snake_case)]
pub struct AuthConfig {
    // -- Crypt
    pub ENCRYPT_KEY: Vec<u8>,
}

impl AuthConfig {
    fn load_from_env() -> envs::Result<AuthConfig> {
        Ok(AuthConfig {
            // -- Crypt
            ENCRYPT_KEY: get_env_b64u_as_u8s("SERVICE_ENCRYPT_KEY")?,
        })
    }
}
