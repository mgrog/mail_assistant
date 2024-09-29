use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

pub type AppResult<T> = Result<T, AppError>;
pub type AppJsonResult<T> = AppResult<Json<T>>;

#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    Internal(anyhow::Error),
    RequestTimeout,
    TooManyRequests,
    Unauthorized(String),
    DbError(sea_orm::error::DbErr),
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
        };
        tracing::error!("Error: {:?}", err.1);

        err.into_response()
    }
}
