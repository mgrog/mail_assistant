use std::collections::HashMap;

use anyhow::anyhow;
use aws_sdk_bedrockruntime::operation::converse::ConverseOutput;
use aws_sdk_bedrockruntime::types::{
    ContentBlock, ConversationRole, Message, Tool, ToolConfiguration, ToolInputSchema,
    ToolSpecification,
};
use aws_smithy_types::Document;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    email::client::EmailMessage,
    server_config::cfg,
    structs::error::{AppError, AppResult, BedrockConverseError},
    ServerState,
};

lazy_static! {
    pub static ref SYSTEM_PROMPT: String = format!(
        "r#
    You are a helpful assistant that categorizes emails.
    Based on the Subject and Body of the email, you will answer with the best fitting category of the following categories: {}
    Please use the categorize_email tool to generate a JSON with the category based on the email subject and body. 
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
    #[derive(Debug)]
    pub static ref TOOL_CONFIG: ToolConfiguration = {
            ToolConfiguration::builder()
                .tools(Tool::ToolSpec(
                        ToolSpecification::builder()
                            .name("categorize_email")
                            .description("Categorize an email into a single category in JSON format")
                            .input_schema(ToolInputSchema::Json(
                                make_tool_schema()
                            ))
                            .build()
                            .unwrap(),
                    ))
                    .build()
                    .expect("Could not build tool configuration")

    };
}

// const AI_ENDPOINT: &str = "https://api.groq.com/openai/v1/chat/completions";

#[derive(Debug, Serialize, Deserialize)]
pub struct PromptResponse {
    pub category: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CategoryPromptResponse {
    pub category: String,
    pub token_usage: i64,
}

fn get_ai_categories() -> Vec<String> {
    cfg.categories.iter().map(|c| c.ai.clone()).collect()
}

pub fn check_config() {
    let system_prompt = SYSTEM_PROMPT.clone();
    let tool_config = TOOL_CONFIG.clone();

    println!(
        "Prompt config: {}",
        json!(
            {
                "system_prompt": system_prompt,
                "tool_config": format!("{:?}", tool_config)
            }
        )
    );

    println!("Prompt config ok!");
}

fn make_tool_schema() -> Document {
    let mut categories_enum = get_ai_categories()
        .into_iter()
        .map(Document::String)
        .collect::<Vec<_>>();
    categories_enum.push(Document::String("Unknown".into()));

    Document::Object(HashMap::<String, Document>::from([
        ("type".into(), "object".into()),
        (
            "properties".into(),
            Document::Object(HashMap::<String, Document>::from([(
                "category".into(),
                Document::Object(HashMap::<String, Document>::from([
                    ("type".into(), "string".into()),
                    (
                        "description".into(),
                        "The category of the email that most aligns with the emails contents"
                            .into(),
                    ),
                    ("enum".into(), Document::Array(categories_enum)),
                ])),
            )])),
        ),
        (
            "required".into(),
            Document::Array(vec![Document::String("category".into())]),
        ),
    ]))
}

pub async fn send_category_prompt_rate_limited(
    server_state: &ServerState,
    email_message: EmailMessage,
) -> AppResult<CategoryPromptResponse> {
    let http_client = &server_state.http_client;
    let aws_client = &server_state.aws_client;
    let subject = email_message.subject.clone().unwrap_or_default();
    let content_str = format!(
        "Subject: {}\nBody: {}",
        subject.clone(),
        email_message.body.unwrap_or_default()
    );

    let resp = aws_client
        .converse()
        .model_id(cfg.model.id.clone())
        .messages(
            Message::builder()
                .role(ConversationRole::User)
                .content(ContentBlock::Text(format!(
                    "Categorize the following email:\n{}",
                    content_str
                )))
                .build()
                .map_err(|_| AppError::Internal(anyhow!("Could not build message".to_string())))?,
        )
        .tool_config(TOOL_CONFIG.clone())
        .send()
        .await;

    let (prompt_answer, usage) = match resp {
        Ok(output) => {
            let text = get_converse_output_text(&output)?;
            let usage = get_converse_token_usage(&output)?;
            Ok((text, usage))
        }
        Err(e) => Err(e
            .as_service_error()
            .map(BedrockConverseError::from)
            .unwrap_or_else(|| BedrockConverseError("Unknown service error".into()))),
    }?;

    println!("Prompt answer: {}", prompt_answer);

    Ok(CategoryPromptResponse {
        category: "test".to_string(),
        token_usage: usage,
    })
}

fn get_converse_output_text(output: &ConverseOutput) -> Result<String, BedrockConverseError> {
    let text = output
        .output()
        .ok_or("no output")?
        .as_message()
        .map_err(|_| "output not a message")?
        .content()
        .first()
        .ok_or("no content in message")?
        .as_text()
        .map_err(|_| "content is not text")?
        .to_string();

    println!("Converse output: {}", text);

    Ok(text)
}

fn get_converse_token_usage(output: &ConverseOutput) -> Result<i64, BedrockConverseError> {
    let usage = output.usage().ok_or("no usage")?.total_tokens();

    Ok(usage as i64)
}
