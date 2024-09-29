use config::{Config, ConfigError};
use google_gmail1::oauth2::{read_application_secret, ApplicationSecret};
use lazy_static::lazy_static;
use serde::Deserialize;
use std::{collections::HashSet, result::Result};

#[derive(Debug, Deserialize)]
pub struct GmailConfig {
    pub client_id: String,
    pub project_id: String,
    pub auth_uri: String,
    pub token_uri: String,
    pub auth_provider_x509_cert_url: String,
    pub client_secret: String,
    pub redirect_uris: Vec<String>,
    pub scopes: Vec<String>,
}

impl GmailConfig {
    pub fn from_file(path: &str) -> Result<Self, ConfigError> {
        let builder = Config::builder()
            .add_source(config::File::with_name(path))
            .build()?;

        builder.try_deserialize()
    }
}

#[derive(Debug, Deserialize)]
struct ConfigFile {
    ai_api_key: String,
    category_labels: Vec<String>,
}

pub struct ServerConfig {
    pub ai_api_key: String,
    pub ai_categories: Vec<String>,
    pub gmail_config: GmailConfig,
    // pub app_secret: ApplicationSecret,
}

impl std::fmt::Display for ServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ai_api_key: {}, ai_categories: {:?}, gmail_config: {:?}",
            self.ai_api_key, self.ai_categories, self.gmail_config
        )
    }
}

lazy_static! {
    pub static ref CONFIG: ServerConfig = {
        let root = env!("CARGO_MANIFEST_DIR");
        let path = format!("{root}/client_secret.toml");
        let gmail_config = GmailConfig::from_file(&path).expect("client_secret.toml is required");
        let path = format!("{root}/config.toml");
        let server_config: ConfigFile = Config::builder()
            .add_source(config::File::with_name(&path))
            .build()
            .expect("config.toml is required")
            .try_deserialize()
            .expect("config.toml is invalid");

        ServerConfig {
            ai_api_key: server_config.ai_api_key,
            ai_categories: server_config.category_labels,
            gmail_config,
        }
    };
}
