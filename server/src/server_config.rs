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
#[allow(dead_code)]
pub struct Category {
    pub content: String,
    pub mail_label: String,
    pub gmail_categories: Vec<String>,
    pub important: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ModelConfig {
    pub id: String,
    pub temperature: f64,
    pub email_confidence_threshold: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PromptLimits {
    pub rate_limit_per_sec: usize,
    pub refill_interval_ms: usize,
    pub refill_amount: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct TokenLimits {
    pub rate_limit_per_min: usize,
    pub refill_interval_ms: usize,
    pub refill_amount: usize,
    pub estimated_token_usage_per_email: usize,
    pub daily_user_quota: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Settings {
    pub training_mode: bool,
    pub email_max_age_days: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ApiConfig {
    pub key: String,
    pub prompt_limits: PromptLimits,
    pub token_limits: TokenLimits,
}

#[derive(Debug, Deserialize)]
struct ConfigFile {
    settings: Settings,
    api: ApiConfig,
    categories: Vec<Category>,
    heuristics: Vec<Category>,
    model: ModelConfig,
}

#[derive(Debug)]
pub struct ServerConfig {
    pub settings: Settings,
    pub api: ApiConfig,
    pub categories: Vec<Category>,
    pub heuristics: Vec<Category>,
    pub gmail_config: GmailConfig,
    pub model: ModelConfig,
}

impl std::fmt::Display for ServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Server Config:\n{:?}\n\nAPI: {:?}\n\nCategories:\n{}\n\nHeuristics:\n{}\n\nGmail Config: {:?}\n\nModel Config: {:?}\n\n",
            self.settings,
            self.api,
            self.categories
                .iter()
                .map(|c| format!("{} -> {}", c.content, c.mail_label))
                .collect::<Vec<_>>().join("\n"),
                self.heuristics
                .iter()
                .map(|c| format!("{} -> {}", c.content, c.mail_label))
                .collect::<Vec<_>>().join("\n"),
            self.gmail_config,
            self.model
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
            settings,
            api,
            categories,
            model,
            heuristics,
        } = cfg_file;

        ServerConfig {
            settings,
            api,
            categories,
            heuristics,
            gmail_config,
            model,
        }
    };
    pub static ref UNKNOWN_CATEGORY: Category = Category {
        content: "Unknown".to_string(),
        mail_label: "mailclerk:uncategorized".to_string(),
        gmail_categories: vec![],
        important: None,
    };
    pub static ref DAILY_SUMMARY_CATEGORY: Category = Category {
        content: "".to_string(),
        mail_label: "mailclerk:daily_summary".to_string(),
        gmail_categories: vec![],
        important: None,
    };
}
