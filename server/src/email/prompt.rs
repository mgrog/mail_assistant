use anyhow::Context;
use lazy_static::lazy_static;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{from_str, json};

use crate::{
    email::client::EmailMessage,
    server_config::CONFIG,
    structs::error::{AppError, AppResult},
    ServerState,
};

lazy_static! {
    static ref SYSTEM_PROMPT: String = format!(
        "r#
    You are a helpful assistant that categorizes emails and responds in JSON.
    Based on the Subject and Body of the email, you will answer with a single category of the following categories: {}
    The answer should be a JSON object with a single key 'category'.
    You cannot choose multiple categories. If you are unsure, you can respond with 'Unknown'.
    The JSON schema should include
    {{
      category: string ({})
    }}
    #",
        get_ai_categories().join(", "),
        [get_ai_categories(), vec!["Unknown".to_string()]].concat().join(", ")
    );
}

const AI_ENDPOINT: &str = "https://api.groq.com/openai/v1/chat/completions";

#[derive(Debug, Serialize, Deserialize)]
pub struct CategoryPromptResponse {
    pub category: String,
    pub token_usage: i64,
}

fn get_ai_categories() -> Vec<String> {
    CONFIG.categories.iter().map(|c| c.ai.clone()).collect()
}

pub async fn send_category_prompt_rate_limited(
    server_state: &ServerState,
    email_message: EmailMessage,
) -> AppResult<CategoryPromptResponse> {
    let http_client = &server_state.http_client;
    let subject = email_message.subject.clone().unwrap_or_default();
    let content_str = format!(
        "Subject: {}\nBody: {}",
        subject.clone(),
        email_message.body.unwrap_or_default()
    );

    #[derive(Debug, Serialize, Deserialize)]
    pub struct PromptResponse {
        pub category: String,
    }

    let resp = http_client
        .post(AI_ENDPOINT)
        .bearer_auth(&CONFIG.ai_api_key)
        .json(&json!(
          {
            "model": "mixtral-8x7b-32768",
            "temperature": CONFIG.model_temperature,
            "messages": [
              {
                "role": "system",
                "content": *SYSTEM_PROMPT
              },
              {
                "role": "user",
                "content": format!("r#
                  Categorize the following email:
                  {}
                 #", content_str)
              }
            ],
            "response_format": { "type": "json_object" }
          }
        ))
        .send()
        .await?
        .json::<serde_json::Value>()
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
        })?;

    println!("Email subject: {}", subject);
    println!("Email snippet: {}", email_message.snippet);
    println!("Prompt response: {:?}", resp);

    #[derive(Debug, Serialize, Deserialize)]
    pub struct PromptUsage {
        pub prompt_tokens: i64,
        pub completion_tokens: i64,
        pub total_tokens: i64,
        pub prompt_time: f64,
        pub completion_time: f64,
        pub total_time: f64,
    }

    let usage = serde_json::from_value::<PromptUsage>(resp["usage"].clone())
        .context("Could not parse usage from resp")?;

    let answer: PromptResponse = from_str(
        resp["choices"]
            .as_array()
            .context("No choices in response")?
            .first()
            .context("No first choice in response")?
            .get("message")
            .context("No message in choice")?
            .get("content")
            .context("No content in message")?
            .as_str()
            .context("Content is not a string")?,
    )
    .context("Could not parse prompt response")?;

    println!("Answer: {:?}", answer);

    Ok(CategoryPromptResponse {
        category: answer.category,
        token_usage: usage.total_tokens,
    })
}
