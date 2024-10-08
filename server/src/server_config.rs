use config::{Config, ConfigError};
use lazy_static::lazy_static;
use serde::Deserialize;
use std::result::Result;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
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

#[derive(Debug, Clone, Deserialize)]
pub struct Category {
    pub ai: String,
    pub mail_label: String,
    pub gmail_categories: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub region: String,
    pub temperature: f64,
}

#[derive(Debug, Deserialize)]
struct ConfigFile {
    ai_api_key: String,
    categories: Vec<Category>,
    prompt_rate_limit_per_min: u64,
    token_rate_limit_per_min: u64,
    daily_user_quota: u64,
    estimated_token_usage_per_email: u64,
    model: ModelConfig,
}

pub struct ServerConfig {
    pub ai_api_key: String,
    pub categories: Vec<Category>,
    pub gmail_config: GmailConfig,
    pub prompt_rate_limit_per_min: u64,
    pub token_rate_limit_per_min: u64,
    pub daily_user_quota: u64,
    pub estimated_token_usage_per_email: u64,
    pub model: ModelConfig,
}

impl std::fmt::Display for ServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ai_api_key: {}, categories: {:?}, gmail_config: {:?}",
            self.ai_api_key, self.categories, self.gmail_config
        )
    }
}

lazy_static! {
    pub static ref cfg: ServerConfig = {
        let root = env!("CARGO_MANIFEST_DIR");
        let path = format!("{root}/client_secret.toml");
        let gmail_config = GmailConfig::from_file(&path).expect("client_secret.toml is required");
        let path = format!("{root}/config.toml");
        let cfg_file: ConfigFile = Config::builder()
            .add_source(config::File::with_name(&path))
            .build()
            .expect("config.toml is required")
            .try_deserialize()
            .expect("config.toml is invalid");

        let ConfigFile {
            ai_api_key,
            categories,
            prompt_rate_limit_per_min,
            token_rate_limit_per_min,
            daily_user_quota,
            estimated_token_usage_per_email,
            model,
        } = cfg_file;

        ServerConfig {
            ai_api_key,
            categories,
            gmail_config,
            prompt_rate_limit_per_min,
            token_rate_limit_per_min,
            daily_user_quota,
            estimated_token_usage_per_email,
            model,
        }
    };
    pub static ref UNKNOWN_CATEGORY: Category = Category {
        ai: "Unknown".to_string(),
        mail_label: "mailclerk:uncategorized".to_string(),
        gmail_categories: vec![],
    };
    pub static ref DAILY_SUMMARY_CATEGORY: Category = Category {
        ai: "".to_string(),
        mail_label: "mailclerk:daily_summary".to_string(),
        gmail_categories: vec![],
    };
}
