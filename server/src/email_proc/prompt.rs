use anyhow::Context;
use lazy_static::lazy_static;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use tokenizers::{tokenizer, Tokenizer};

use crate::{
    email_client::EmailMessage,
    server_config::CONFIG,
    structs::error::{AppError, AppResult},
    ServerState,
};

use super::EmailProcessor;

lazy_static! {
  static ref SYSTEM_PROMPT: String = format!("r#
    You are a helpful assistant capable of categorizing emails into different categories that responds in JSON.
    The JSON schema should include
    {{
      category: string ({})
    }}
    #", CONFIG.ai_categories.join(", "));
}

const AI_ENDPOINT: &str = "https://api.groq.com/openai/v1/chat/completions";

#[derive(Debug, Serialize, Deserialize)]
pub struct PromptResponse {
    pub category: String,
}

pub async fn send_category_prompt(
    server_state: ServerState,
    user_token_count: AtomicU64,
    email_message: EmailMessage,
) -> AppResult<PromptResponse> {
    let http_client = &server_state.http_client;
    let tokenizer = &server_state.tokenizer;
    let content_str = format!(
        "Subject: {}\nBody: {}",
        email_message.subject.unwrap_or_default(),
        email_message.body.unwrap_or_default()
    );
    let content_token_count = count_tokens(tokenizer, &content_str)?;
    // Add to specific user token count for quota
    user_token_count.fetch_add(content_token_count, Relaxed);
    // Add to total token count
    server_state.add_global_token_count(content_token_count);

    http_client
        .post(AI_ENDPOINT)
        .json(&json!(
          {
            "model": "llama-3.1-70b-versatile",
            "messages": [
              {
                "role": "system",
                "content": *SYSTEM_PROMPT
              },
              {
                "role": "user",
                "content": format!("r#
                  Categorize the following email into exactly one of the following categories: {}
                  If no category is applicable, please respond with 'UNCATEGORIZED'.

                  {}
                 #", CONFIG.ai_categories.join(", "), content_str)
              }
            ]
          }
        ))
        .send()
        .await?
        .json()
        .await
        .map_err(|e| {
            if let Some(status) = e.status() {
                match status {
                    StatusCode::BAD_REQUEST => AppError::BadRequest(e.to_string()),
                    StatusCode::REQUEST_TIMEOUT => AppError::RequestTimeout,
                    StatusCode::TOO_MANY_REQUESTS => AppError::TooManyRequests,
                    _ => AppError::Internal(e.into()),
                }
            } else {
                AppError::Internal(e.into())
            }
        })
}

pub fn count_tokens(tokenizer: &Tokenizer, input: &str) -> AppResult<u64> {
    let tokens = tokenizer
        .encode(input, false)
        .map_err(|_| anyhow::anyhow!("Failed to tokenize input: {input}"))?;
    let count: u64 = tokens
        .len()
        .try_into()
        .map_err(|_| anyhow::anyhow!("Token count overflow"))?;

    Ok(count)
}
