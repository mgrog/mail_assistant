use std::{collections::HashSet, str::FromStr};

use axum::{
    extract::{Query, State},
    Json,
};
use lib_email_clients::gmail::AccessScopes;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::{
    email::client::{EmailClient, MessageListOptions},
    error::{AppError, AppJsonResult},
    model::{
        response::{CheckAccountConnectionResponse, GmailAccountConnectionStatus, GoogleTokenInfo},
        user::{AccountAccess, UserCtrl},
    },
    server_config::cfg,
    HttpClient,
};

async fn _missing_scopes(
    http_client: &HttpClient,
    access_token: &str,
) -> Result<Vec<AccessScopes>, ScopeError> {
    let required_scopes = &cfg.gmail_config.scopes;
    let mut missing_scopes = vec![];

    let resp = http_client
        .get("https://www.googleapis.com/oauth2/v1/tokeninfo")
        .query(&[("access_token", access_token)])
        .send()
        .await?;

    let resp = resp.json::<serde_json::Value>().await?;

    if resp.get("error").is_some() {
        let error = resp.get("error").unwrap().as_str().unwrap();
        match error {
            "invalid_token" => return Err(ScopeError::InvalidToken),
            _ => return Err(ScopeError::UnexpectedError),
        }
    }

    let data = serde_json::from_value::<GoogleTokenInfo>(resp.clone())
        .map_err(|e| {
            tracing::error!("Unexpected token info response: {:?}", resp);
            e
        })
        .map_err(|_| ScopeError::UnexpectedError)?;

    let scopes = data.scope.split(' ').collect::<HashSet<&str>>();
    for scope in required_scopes {
        if !scopes.contains(scope.as_str()) {
            missing_scopes.push(AccessScopes::from_str(scope).unwrap());
        }
    }

    Ok(missing_scopes)
}

async fn _profile_ok(email_client: &EmailClient) -> bool {
    let profile_result = email_client.get_profile().await;
    matches!(profile_result, Ok(profile) if profile.email_address.is_some())
}

async fn _get_latest_message_id(email_client: &EmailClient) -> anyhow::Result<Option<String>> {
    let options = MessageListOptions {
        max_results: Some(1),
        ..Default::default()
    };
    let response = email_client.get_message_list(options).await?;

    Ok(response
        .messages
        .and_then(|msgs| msgs.first().and_then(|m| m.id.clone())))
}

async fn _read_messages_ok(email_client: &EmailClient) -> bool {
    let options = MessageListOptions {
        max_results: Some(10),
        ..Default::default()
    };

    email_client.get_message_list(options).await.is_ok()
}

//? Maybe get rid of message insert
// async fn _insert_messages_ok(email_client: &EmailClient) -> bool {
//     unimplemented!()
// }

async fn _labels_ok(email_client: &EmailClient) -> bool {
    email_client.get_labels().await.is_ok()
}

#[derive(Deserialize, Serialize)]
pub struct AccountConnectionQuery {
    email: String,
}

pub async fn check_account_connection(
    State(http_client): State<HttpClient>,
    State(conn): State<DatabaseConnection>,
    Query(query): Query<AccountConnectionQuery>,
) -> AppJsonResult<CheckAccountConnectionResponse> {
    let user_access = match UserCtrl::get_with_account_access_by_email(&conn, &query.email).await {
        Ok(user) => user,
        Err(AppError::NotFound(_)) => {
            return Ok(Json(CheckAccountConnectionResponse {
                email: query.email,
                result: GmailAccountConnectionStatus::NotConnected,
            }))
        }
        Err(e) => {
            return Err(e);
        }
    };

    let email_client =
        EmailClient::new(http_client.clone(), conn.clone(), user_access.clone()).await?;

    let mut failed_checks = vec![];

    let missing_scopes = match _missing_scopes(&http_client, &user_access.access_token()?).await {
        Ok(missing_scopes) => missing_scopes,
        Err(_) => {
            // If this fails, its likely the token is invalid
            return Ok(Json(CheckAccountConnectionResponse {
                email: query.email,
                result: GmailAccountConnectionStatus::NotConnected,
            }));
        }
    };

    if !missing_scopes.is_empty() {
        return Ok(Json(CheckAccountConnectionResponse {
            email: query.email,
            result: GmailAccountConnectionStatus::MissingScopes { missing_scopes },
        }));
    }

    if !_profile_ok(&email_client).await {
        failed_checks.push("profile".to_string());
    }

    if !_read_messages_ok(&email_client).await {
        failed_checks.push("read_messages".to_string());
    }

    if !_labels_ok(&email_client).await {
        failed_checks.push("labels".to_string());
    }

    if !failed_checks.is_empty() {
        return Ok(Json(CheckAccountConnectionResponse {
            email: query.email,
            result: GmailAccountConnectionStatus::FailedChecks { failed_checks },
        }));
    }

    Ok(Json(CheckAccountConnectionResponse {
        email: query.email,
        result: GmailAccountConnectionStatus::Good,
    }))
}

pub enum ScopeError {
    InvalidToken,
    NetworkError,
    UnexpectedError,
}

impl From<reqwest::Error> for ScopeError {
    fn from(_: reqwest::Error) -> Self {
        ScopeError::NetworkError
    }
}
