use std::sync::LazyLock;

use axum::http::HeaderMap;
use chrono::Utc;
use http::header::SET_COOKIE;
use jsonwebtoken::{DecodingKey, EncodingKey, Header};
use serde::{Deserialize, Serialize};

static KEYS: LazyLock<Keys> = LazyLock::new(|| {
    let secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");
    Keys::new(&secret)
});

const COMPANY: &str = "mailclerk.io";

pub const SHORT_TTL: usize = 5 * 60; // 5 minutes
pub const LONG_TTL: usize = 24 * 60 * 60; // 24 hours
const COOKIE_NAME: &str = "session";
const DOMAIN: &str = "localhost";

pub fn generate_redirect_jwt() -> Result<String, AuthError> {
    let claims = Claims {
        sub: "redirect".to_string(),
        company: COMPANY.to_string(),
        exp: Utc::now().timestamp() as usize + SHORT_TTL,
    };

    jsonwebtoken::encode(&Header::default(), &claims, &KEYS.encoding)
        .map_err(|_| AuthError::TokenCreation)
}

pub fn generate_redirect_auth_headers() -> Result<HeaderMap, AuthError> {
    let token = generate_redirect_jwt()?;
    let mut headers = HeaderMap::new();
    let cookie = format!("{COOKIE_NAME}={token}; Domain={DOMAIN} SameSite=Lax; Path=/");
    headers.insert(SET_COOKIE, cookie.parse().unwrap());

    Ok(headers)
}

struct Keys {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

impl Keys {
    fn new(secret: &str) -> Self {
        Self {
            encoding: EncodingKey::from_base64_secret(secret)
                .expect("Secret was invalid for encoding key"),
            decoding: DecodingKey::from_base64_secret(secret)
                .expect("Secret was invalid for decoding key"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Claims {
    sub: String,
    company: String,
    exp: usize,
}

#[derive(Debug)]
pub(crate) enum AuthError {
    WrongCredentials,
    MissingCredentials,
    TokenCreation,
    InvalidToken,
}
