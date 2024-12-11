use anyhow::anyhow;
use anyhow::Context;
use lazy_static::lazy_static;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::rate_limiters;
use crate::HttpClient;
use crate::{
    email::client::EmailMessage,
    error::{AppError, AppResult},
    server_config::cfg,
};

lazy_static! {
    static ref SYSTEM_PROMPT: String = format!(
        "r#
    You are a helpful assistant that can categorize emails such as the categories inside the square brackets below.
    [{}]
    You should try to choose a single category from the above, along with its confidence score. 
    You will only respond with a JSON object with the keys category and confidence. Do not provide explanations or multiple categories.

    #", get_ai_categories().join(", "));
}

const AI_ENDPOINT: &str = "https://api.mistral.ai/v1/chat/completions";

fn get_system_prompt() -> String {
    const SYSTEM_INTRO: &str = "You are a helpful assistant that can categorize emails such as the categories inside the square brackets below.";
    const SYSTEM_OUTRO: &str = concat!(
    "You should try to choose a single category from the above, along with its confidence score.",
    "You will only respond with a JSON object with the keys category and confidence. Do not provide explanations or multiple categories.");

    format!(
        "{}\n{}\n{}",
        SYSTEM_INTRO,
        get_ai_categories().join(", "),
        SYSTEM_OUTRO
    )
}

fn get_ai_categories() -> Vec<String> {
    cfg.categories.iter().map(|c| c.content.clone()).collect()
}

pub async fn send_category_prompt(
    http_client: &HttpClient,
    rate_limiters: &rate_limiters::RateLimiters,
    email_message: &EmailMessage,
) -> AppResult<CategoryPromptResponse> {
    let subject = email_message.subject.as_ref().map_or("", |s| s.as_str());
    let body = email_message.body.as_ref().map_or("", |s| s.as_str());
    let email_content_str = format!("<subject>{}</subject>\n<body>{}</body>", subject, body);

    let resp = http_client
        .post(AI_ENDPOINT)
        .bearer_auth(&cfg.api.key)
        .json(&json!(
          {
            "model": &cfg.model.id,
            "temperature": cfg.model.temperature,
            "messages": [
              {
                "role": "system",
                "content": *SYSTEM_PROMPT
              },
              {
                "role": "user",
                "content": format!("r#
                  Categorize the following email based on the email subject between the <subject> tags and the email body between the <body> tags.
                  {}
                 #", email_content_str)
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

    let parsed = serde_json::from_value::<ChatApiResponseOrError>(resp.clone())
        .context(format!("Could not parse chat response: {}", resp))?;

    let parsed = match parsed {
        ChatApiResponseOrError::Error(error) => {
            if error.message == "Requests rate limit exceeded" {
                rate_limiters.trigger_backoff();
            }
            return Err(anyhow!("Chat API error: {:?}", error).into());
        }
        ChatApiResponseOrError::Response(parsed) => parsed,
    };

    let (category, confidence, usage) = {
        let choice = parsed.choices.first().context("No choices in response")?;
        let usage = parsed.usage;
        match serde_json::from_str(choice.message.content.as_str()) {
            Ok(AnswerJson {
                category,
                confidence,
            }) => Ok::<_, AppError>((category, confidence, usage)),
            Err(_) => {
                println!("Could not parse JSON response, parsing manually...");
                static RE_CAT: Lazy<Regex> =
                    Lazy::new(|| Regex::new(r#""category": "(.*)""#).unwrap());
                static RE_CONF: Lazy<Regex> =
                    Lazy::new(|| Regex::new(r#""confidence": (.*)"#).unwrap());
                let category = match RE_CAT.captures(&choice.message.content) {
                    Some(caps) => {
                        let category = caps
                            .get(1)
                            .context("No category in response")?
                            .as_str()
                            .to_string();

                        Ok(category)
                    }
                    None => Err(anyhow!(
                        "Could not parse category from response: {:?}",
                        choice
                    )),
                }?;

                let confidence = match RE_CONF.captures(&choice.message.content) {
                    Some(caps) => {
                        let confidence = caps
                            .get(1)
                            .context("No confidence in response")?
                            .as_str()
                            .parse::<f32>()
                            .context("Could not parse confidence")?;

                        Ok(confidence)
                    }
                    None => Err(anyhow!(
                        "Could not parse confidence from response: {:?}",
                        choice
                    )),
                }?;

                Ok((category, confidence, usage))
            }
        }
    }?;

    // -- DEBUG
    // println!("Email from: {:?}", email_message.from);
    // println!("Email subject: {}", subject);
    // println!("Email snippet: {}", email_message.snippet);
    // println!("Email body: {}", body.chars().take(400).collect::<String>());
    // println!("Answer: {}, Confidence: {}", category, confidence);
    // -- DEBUG

    Ok(CategoryPromptResponse {
        category,
        confidence,
        token_usage: usage.total_tokens,
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CategoryPromptResponse {
    pub category: String,
    pub confidence: f32,
    pub token_usage: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnswerJson {
    pub category: String,
    pub confidence: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PromptUsage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ModelLength,
    Error,
    ToolCalls,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatChoice {
    pub index: i32,
    pub message: ChatMessage,
    pub finish_reason: FinishReason,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatApiResponse {
    pub choices: Vec<ChatChoice>,
    pub usage: PromptUsage,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatApiError {
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatApiResponseOrError {
    Response(ChatApiResponse),
    Error(ChatApiError),
}
