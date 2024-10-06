extern crate google_gmail1 as gmail;
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use anyhow::Context;
use axum::{
    extract::{Query, State},
    Json,
};
use chrono::DateTime;
use entity::user_session;
use google_gmail1::oauth2::authenticator_delegate::{
    DefaultInstalledFlowDelegate, InstalledFlowDelegate,
};
use sea_orm::{sea_query::OnConflict, ActiveValue, EntityTrait};
use serde::Deserialize;
use serde_json::json;
use user_session::Column::*;

use crate::{
    email::EmailClient,
    server_config::{GmailConfig, CONFIG},
    structs::{
        error::{AppError, AppJsonResult, AppResult},
        response::{GmailApiRefreshTokenResponse, GmailApiTokenResponse},
    },
    HttpClient, ServerState,
};

async fn browser_user_url(url: &str, need_code: bool) -> Result<String, String> {
    let def_delegate = DefaultInstalledFlowDelegate;
    def_delegate.present_user_url(url, need_code).await
}

#[derive(Clone)]
struct InstalledFlowBrowserDelegate {
    url: Arc<Mutex<String>>,
}

impl InstalledFlowDelegate for InstalledFlowBrowserDelegate {
    fn present_user_url<'a>(
        &'a self,
        url: &'a str,
        need_code: bool,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        self.url.lock().unwrap().push_str(url);
        Box::pin(browser_user_url(url, need_code))
    }

    fn redirect_uri(&self) -> Option<&str> {
        None
    }
}

// pub async fn handler_auth_gmail(
//     State(state): State<ServerState>,
// ) -> AppJsonResult<serde_json::Value> {
//     let secret = CONFIG.app_secret.clone();
//     let flow_delegate = InstalledFlowBrowserDelegate {
//         url: Arc::new(Mutex::new(String::new())),
//     };
//     let redirect_uri = flow_delegate.url.clone();
//     let auth = InstalledFlowAuthenticator::builder(
//         secret,
//         oauth2::InstalledFlowReturnMethod::HTTPPortRedirect(8080),
//     )
//     .flow_delegate(Box::new(flow_delegate))
//     .build()
//     .await
//     .context("Failed to create authenticator")?;

//     Ok(Json(json!({
//         "url": redirect_uri.lock().unwrap().clone()
//     })))
// }

pub async fn handler_auth_gmail(
    State(http_client): State<HttpClient>,
) -> AppJsonResult<serde_json::Value> {
    let GmailConfig {
        auth_uri,
        client_id,
        redirect_uris,
        scopes,
        ..
    } = &CONFIG.gmail_config;

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
    } = &CONFIG.gmail_config;

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
        EmailClient::new(state.http_client.clone(), resp.access_token.clone()).await?;
    let profile = email_client.get_profile().await?;
    println!("Profile: {:?}", profile);
    let email = profile
        .email_address
        .context("Profile email not found. An email address is required")?;

    let session = user_session::ActiveModel {
        id: ActiveValue::NotSet,
        email: ActiveValue::Set(email),
        access_token: ActiveValue::Set(resp.access_token.clone()),
        refresh_token: ActiveValue::Set(resp.refresh_token.clone()),
        expires_at: ActiveValue::Set(DateTime::from(
            chrono::Utc::now() + chrono::Duration::seconds(resp.expires_in as i64),
        )),
        created_at: ActiveValue::NotSet,
        updated_at: ActiveValue::NotSet,
    };
    user_session::Entity::insert(session)
        .on_conflict(
            OnConflict::column(Email)
                .update_columns([AccessToken, RefreshToken, ExpiresAt, UpdatedAt])
                .to_owned(),
        )
        .exec(&state.conn)
        .await?;

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
    } = &CONFIG.gmail_config;

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
