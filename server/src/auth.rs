//! Credentials, sessions and the `AuthUser` extractor.
//!
//! Requests authenticate with either a session cookie (browsers) or an
//! `Authorization: Bearer` header carrying a session token or a bot token.

use actix_web::{
    FromRequest, HttpRequest,
    cookie::{Cookie, SameSite, time::Duration},
    dev::Payload,
    http::header::AUTHORIZATION,
    web,
};
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng as PwRng},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use futures_util::future::LocalBoxFuture;
use rand::{RngCore, rngs::OsRng};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    AppState,
    config::Config,
    error::{ApiError, ApiResult},
};

pub const SESSION_COOKIE: &str = "methods_session";
pub const BOT_TOKEN_PREFIX: &str = "mth_";

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct AuthUser {
    pub id: Uuid,
    pub kind: String,
    pub display_name: String,
}

impl AuthUser {
    pub fn is_bot(&self) -> bool {
        self.kind == "bot"
    }
}

pub fn random_token(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

pub fn hash_password(password: &str) -> ApiResult<String> {
    let salt = SaltString::generate(&mut PwRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| ApiError::Other(anyhow::anyhow!("hashing password: {e}")))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
}

pub async fn create_session(db: &PgPool, user_id: Uuid, days: i64) -> ApiResult<String> {
    let token = random_token(32);
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, expires_at) VALUES ($1, $2, now() + make_interval(days => $3))",
    )
    .bind(hash_token(&token))
    .bind(user_id)
    .bind(days as i32)
    .execute(db)
    .await?;
    // Opportunistic clean-up of expired sessions and abandoned OIDC logins.
    let _ = sqlx::query("DELETE FROM sessions WHERE expires_at < now()").execute(db).await;
    let _ = sqlx::query("DELETE FROM oidc_states WHERE created_at < now() - interval '1 hour'")
        .execute(db)
        .await;
    Ok(token)
}

pub fn session_cookie(token: String, config: &Config) -> Cookie<'static> {
    Cookie::build(SESSION_COOKIE, token)
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(config.cookie_secure)
        .max_age(Duration::days(config.session_days))
        .finish()
}

pub fn clear_session_cookie(config: &Config) -> Cookie<'static> {
    let mut cookie = session_cookie(String::new(), config);
    cookie.make_removal();
    cookie
}

/// Creates a bot access token. Returns `(plaintext, prefix, hash)`; only the
/// hash is stored, the plaintext is shown to the user once.
pub fn new_bot_token() -> (String, String, String) {
    let token = format!("{BOT_TOKEN_PREFIX}{}", random_token(30));
    let prefix = token.chars().take(BOT_TOKEN_PREFIX.len() + 6).collect();
    let hash = hash_token(&token);
    (token, prefix, hash)
}

pub async fn authenticate(db: &PgPool, token: &str) -> ApiResult<Option<AuthUser>> {
    let hash = hash_token(token);
    let user = if token.starts_with(BOT_TOKEN_PREFIX) {
        sqlx::query_as::<_, AuthUser>(
            "UPDATE bot_tokens t SET last_used_at = now()
             FROM users u
             WHERE t.token_hash = $1 AND t.revoked_at IS NULL AND u.id = t.bot_id
             RETURNING u.id, u.kind, u.display_name",
        )
        .bind(hash)
        .fetch_optional(db)
        .await?
    } else {
        sqlx::query_as::<_, AuthUser>(
            "SELECT u.id, u.kind, u.display_name FROM sessions s JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = $1 AND s.expires_at > now()",
        )
        .bind(hash)
        .fetch_optional(db)
        .await?
    };
    Ok(user)
}

/// The token presented with the request, from the Authorization header or the
/// session cookie.
pub fn request_token(req: &HttpRequest) -> Option<String> {
    let bearer = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")))
        .map(|v| v.trim().to_string());
    bearer.or_else(|| req.cookie(SESSION_COOKIE).map(|c| c.value().to_string()))
}

impl FromRequest for AuthUser {
    type Error = ApiError;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let state = req.app_data::<web::Data<AppState>>().cloned();
        let token = request_token(req);
        Box::pin(async move {
            let state = state.ok_or_else(|| ApiError::Other(anyhow::anyhow!("app state missing")))?;
            let token = token.filter(|t| !t.is_empty()).ok_or(ApiError::Unauthorized)?;
            authenticate(&state.db, &token).await?.ok_or(ApiError::Unauthorized)
        })
    }
}
