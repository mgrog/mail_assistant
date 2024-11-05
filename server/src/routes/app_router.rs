use axum::{http::StatusCode, response::IntoResponse, routing::get, Router};
use tower_cookies::CookieManagerLayer;
use tower_http::cors::CorsLayer;

use crate::{request_tracing, ServerState};

pub struct AppRouter;

impl AppRouter {
    pub fn create(state: ServerState) -> Router {
        Router::new()
            .route("/", get(|| async { "Mailclerk server" }))
            .route("/auth/gmail", get(super::auth::handler_auth_gmail))
            .route(
                "/auth/callback",
                get(super::auth::handler_auth_gmail_callback),
            )
            .route(
                "/auth_token/callback",
                get(super::auth::handler_auth_token_callback),
            )
            .layer(request_tracing::trace_with_request_id_layer())
            .layer(CorsLayer::permissive())
            .layer(CookieManagerLayer::new())
            .with_state(state.clone())
            .fallback(handler_404)
    }
}

pub async fn handler_404() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "Route does not exist")
}
