use aws_sdk_bedrockruntime::operation::converse::ConverseError;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use num_derive::{FromPrimitive, ToPrimitive};
use serde_json::json;
use sqlx::error::DatabaseError;

use crate::db_core::prelude::*;
use crate::server_config::cfg;

pub type AppResult<T> = Result<T, AppError>;
pub type AppJsonResult<T> = AppResult<Json<T>>;

#[derive(Debug)]
#[allow(dead_code)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    Internal(anyhow::Error),
    RequestTimeout,
    TooManyRequests,
    Unauthorized(String),
    DbError(sea_orm::error::DbErr),
    Conflict(String),
    AiPrompt(BedrockConverseError),
}

impl From<anyhow::Error> for AppError {
    fn from(error: anyhow::Error) -> Self {
        AppError::Internal(error)
    }
}

impl From<reqwest::Error> for AppError {
    fn from(error: reqwest::Error) -> Self {
        match error.status() {
            Some(StatusCode::BAD_REQUEST) => AppError::BadRequest(error.to_string()),
            Some(StatusCode::REQUEST_TIMEOUT) => AppError::RequestTimeout,
            Some(StatusCode::TOO_MANY_REQUESTS) => AppError::TooManyRequests,
            _ => AppError::Internal(error.into()),
        }
    }
}

impl From<sea_orm::error::DbErr> for AppError {
    fn from(error: sea_orm::error::DbErr) -> Self {
        AppError::DbError(error)
    }
}

impl From<BedrockConverseError> for AppError {
    fn from(error: BedrockConverseError) -> Self {
        AppError::AiPrompt(error)
    }
}

impl From<&ConverseError> for AppError {
    fn from(error: &ConverseError) -> Self {
        AppError::AiPrompt(error.into())
    }
}

#[derive(Debug)]
pub struct BedrockConverseError(pub String);
impl std::fmt::Display for BedrockConverseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Can't invoke '{}'. Reason: {}", cfg.model.id, self.0)
    }
}
impl std::error::Error for BedrockConverseError {}
impl From<&str> for BedrockConverseError {
    fn from(value: &str) -> Self {
        BedrockConverseError(value.to_string())
    }
}
impl From<&ConverseError> for BedrockConverseError {
    fn from(value: &ConverseError) -> Self {
        BedrockConverseError::from(match value {
            ConverseError::ModelTimeoutException(_) => "Model took too long",
            ConverseError::ModelNotReadyException(_) => "Model is not ready",
            _ => {
                tracing::error!("Unknown error: {:?}", value);
                "Unknown"
            }
        })
    }
}

// This centralizes all different errors from our app in one place
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let err = match self {
            AppError::BadRequest(error) => (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": {
                    "code": StatusCode::BAD_REQUEST.as_u16(),
                    "message": error
                }})),
            ),
            AppError::NotFound(msg) => (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "code": StatusCode::NOT_FOUND.as_u16(),
                    "message": msg
                })),
            ),
            AppError::Internal(e) => {
                tracing::error!("error msg: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": {
                        "code": StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                        "message": "Internal server error"
                    }})),
                )
            }
            AppError::RequestTimeout => (
                StatusCode::REQUEST_TIMEOUT,
                Json(json!({
                    "error": {
                        "code": StatusCode::REQUEST_TIMEOUT.as_u16(),
                        "message": "Request took too long"
                    }
                })),
            ),
            AppError::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({
                    "error": {
                        "code": StatusCode::TOO_MANY_REQUESTS.as_u16(),
                        "message": "Too many requests"
                    }
                })),
            ),
            AppError::Unauthorized(error) => (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": {
                        "code": StatusCode::UNAUTHORIZED.as_u16(),
                        "message": error
                    }
                })),
            ),
            AppError::DbError(err) => {
                tracing::error!("Error: {:?}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": {
                        "code": StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                        "message": "Database error"
                    }})),
                )
            }
            AppError::Conflict(msg) => (
                StatusCode::CONFLICT,
                Json(json!({
                    "code": StatusCode::CONFLICT.as_u16(),
                    "message": msg
                })),
            ),
            AppError::AiPrompt(err) => {
                tracing::error!("Error: {:?}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": {
                        "code": StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                        "message": "AI prompt error"
                    }})),
                )
            }
        };
        tracing::error!("Error: {:?}", err.1);

        err.into_response()
    }
}

#[allow(clippy::borrowed_box)]
fn get_code(error: &Box<dyn DatabaseError>) -> Option<u32> {
    error.code().and_then(|c| c.parse::<u32>().ok())
}

pub fn extract_database_error_code(err: &sea_orm::error::DbErr) -> Option<u32> {
    match err {
        sea_orm::error::DbErr::Query(sea_orm::error::RuntimeErr::SqlxError(
            sqlx::Error::Database(error),
        )) => get_code(error),
        _ => None,
    }
}

#[derive(FromPrimitive, ToPrimitive, Debug, PartialEq, Eq)]
pub enum DatabaseErrorCode {
    UniqueViolation = 23505,
}
