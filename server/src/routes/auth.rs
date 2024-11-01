extern crate google_gmail1 as gmail;

use crate::db_core::{prelude::*, queries::configure_default_user_settings};
use anyhow::Context;
use axum::{
    extract::{Query, State},
    Json,
};
use chrono::DateTime;
use entity::user_account_access::Column::*;
use serde::Deserialize;
use serde_json::json;

use crate::{
    email::client::EmailClient,
    model::{
        error::{AppError, AppJsonResult, AppResult},
        response::{GmailApiRefreshTokenResponse, GmailApiTokenResponse},
    },
    server_config::{cfg, GmailConfig},
    HttpClient, ServerState,
};

pub async fn handler_auth_gmail(
    State(http_client): State<HttpClient>,
) -> AppJsonResult<serde_json::Value> {
    let GmailConfig {
        auth_uri,
        client_id,
        redirect_uris,
        scopes,
        ..
    } = &cfg.gmail_config;

    let req = http_client
        .get(auth_uri)
        .query(&[
            ("client_id", client_id.as_str()),
            ("redirect_uri", redirect_uris[0].as_str()),
            ("response_type", "code"),
            ("scope", scopes.join(" ").as_str()),
            ("access_type", "offline"),
            ("prompt", "select_account"),
        ])
        .build()?;

    Ok(Json(json!({
        "url": req.url().to_string()
    })))
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub error: Option<String>,
    pub scope: Option<String>,
}

pub async fn handler_auth_gmail_callback(
    State(state): State<ServerState>,
    Query(query): Query<CallbackQuery>,
) -> AppJsonResult<serde_json::Value> {
    tracing::info!("Callback query: {:?}", query);
    if let Some(error) = query.error {
        return Err(AppError::Unauthorized(error));
    }
    if query.code.is_none() {
        return Err(AppError::BadRequest("Missing code".to_string()));
    }
    let code = query.code.as_ref().unwrap();

    let GmailConfig {
        token_uri,
        client_id,
        client_secret,
        redirect_uris,
        ..
    } = &cfg.gmail_config;

    let resp = state
        .http_client
        .post(token_uri)
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", code.as_str()),
            ("redirect_uri", redirect_uris[0].as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await?;

    let resp: serde_json::Value = resp.json().await?;
    let resp: GmailApiTokenResponse = serde_json::from_value(resp.clone()).map_err(|_| {
        tracing::error!("Failed to parse response: {:?}", resp);
        AppError::BadRequest(resp.to_string())
    })?;

    let email_client =
        EmailClient::from_access_code(state.http_client.clone(), resp.access_token.clone());
    let profile = email_client.get_profile().await?;
    // -- DEBUG
    // println!("Profile: {:?}", profile);
    // -- DEBUG
    let email = profile
        .email_address
        .context("Profile email not found. An email address is required")?;

    User::insert(user::ActiveModel {
        id: ActiveValue::NotSet,
        email: ActiveValue::Set(email.clone()),
        created_at: ActiveValue::NotSet,
        updated_at: ActiveValue::NotSet,
        subscription_status: ActiveValue::NotSet,
        last_payment_attempt_at: ActiveValue::NotSet,
        last_successful_payment_at: ActiveValue::NotSet,
        last_sync: ActiveValue::NotSet,
    })
    .on_conflict_do_nothing()
    .exec(&state.conn)
    .await?;

    let account_access = user_account_access::ActiveModel {
        id: ActiveValue::NotSet,
        user_email: ActiveValue::Set(email),
        access_token: ActiveValue::Set(resp.access_token.clone()),
        refresh_token: ActiveValue::Set(resp.refresh_token.clone()),
        expires_at: ActiveValue::Set(DateTime::from(
            chrono::Utc::now() + chrono::Duration::seconds(resp.expires_in as i64),
        )),
        created_at: ActiveValue::NotSet,
        updated_at: ActiveValue::NotSet,
    };
    let user_account_access = UserAccountAccess::insert(account_access)
        .on_conflict(
            OnConflict::column(UserEmail)
                .update_columns([AccessToken, RefreshToken, ExpiresAt, UpdatedAt])
                .to_owned(),
        )
        .exec_with_returning(&state.conn)
        .await?;

    match configure_default_user_settings(&state, &user_account_access.user_email).await {
        Ok(_) => {}
        Err(AppError::Conflict(_)) => {
            tracing::info!("User settings already exists");
        }
        Err(e) => {
            return Err(e);
        }
    }

    Ok(Json(json!({
        "message": "Login success",
    })))
}

pub async fn handler_auth_token_callback() -> AppJsonResult<serde_json::Value> {
    Ok(Json(json!({
        "message": "Login success"
    })))
}

pub async fn exchange_refresh_token(
    http_client: reqwest::Client,
    refresh_token: String,
) -> AppResult<GmailApiRefreshTokenResponse> {
    let GmailConfig {
        token_uri,
        client_id,
        client_secret,
        ..
    } = &cfg.gmail_config;

    let resp = http_client
        .post(token_uri)
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("refresh_token", refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await?;

    let resp = resp.json().await?;
    Ok(resp)
}
