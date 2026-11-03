//! Login with any OpenID Connect provider (authorization code flow + PKCE).
//!
//! Users are matched by (issuer, subject), then by verified email, and
//! created on first login.

use actix_web::{HttpResponse, http::header::LOCATION, web};
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, Scope, TokenResponse,
    core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata},
    reqwest,
};
use serde::Deserialize;
use tokio::sync::OnceCell;
use uuid::Uuid;

use crate::{
    AppState, auth,
    config::OidcConfig,
    error::{ApiError, ApiResult},
};

pub struct Oidc {
    config: OidcConfig,
    redirect_url: String,
    http: reqwest::Client,
    metadata: OnceCell<CoreProviderMetadata>,
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::Other(anyhow::anyhow!("oidc: {e}"))
}

impl Oidc {
    pub fn new(config: OidcConfig, public_url: &str) -> anyhow::Result<Self> {
        Ok(Oidc {
            config,
            redirect_url: format!("{public_url}/api/auth/oidc/callback"),
            http: reqwest::ClientBuilder::new().redirect(reqwest::redirect::Policy::none()).build()?,
            metadata: OnceCell::new(),
        })
    }

    /// Provider discovery happens on first use, so the server starts even if
    /// the provider is briefly unreachable.
    async fn metadata(&self) -> ApiResult<&CoreProviderMetadata> {
        self.metadata
            .get_or_try_init(|| async {
                let issuer = IssuerUrl::new(self.config.issuer.clone()).map_err(internal)?;
                CoreProviderMetadata::discover_async(issuer, &self.http).await.map_err(internal)
            })
            .await
    }
}

fn redirect(to: &str) -> HttpResponse {
    HttpResponse::Found().insert_header((LOCATION, to.to_string())).finish()
}

#[derive(Deserialize)]
pub struct StartQuery {
    return_to: Option<String>,
}

pub async fn start(state: web::Data<AppState>, q: web::Query<StartQuery>) -> ApiResult<HttpResponse> {
    let oidc = state.oidc.as_ref().ok_or(ApiError::NotFound)?;
    let metadata = oidc.metadata().await?.clone();
    let client = CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(oidc.config.client_id.clone()),
        oidc.config.client_secret.clone().map(ClientSecret::new),
    )
    .set_redirect_uri(RedirectUrl::new(oidc.redirect_url.clone()).map_err(internal)?);

    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let (url, csrf, nonce) = client
        .authorize_url(CoreAuthenticationFlow::AuthorizationCode, CsrfToken::new_random, Nonce::new_random)
        .add_scope(Scope::new("email".into()))
        .add_scope(Scope::new("profile".into()))
        .set_pkce_challenge(challenge)
        .url();

    // Only allow returning to a path on this site.
    let return_to = q
        .return_to
        .as_deref()
        .filter(|p| p.starts_with('/') && !p.starts_with("//"))
        .unwrap_or("/");
    sqlx::query("INSERT INTO oidc_states (state, nonce, pkce_verifier, return_to) VALUES ($1, $2, $3, $4)")
        .bind(csrf.secret())
        .bind(nonce.secret())
        .bind(verifier.secret())
        .bind(return_to)
        .execute(&state.db)
        .await?;
    Ok(redirect(url.as_str()))
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

pub async fn callback(state: web::Data<AppState>, q: web::Query<CallbackQuery>) -> ApiResult<HttpResponse> {
    let fail = |msg: &str| Ok(redirect(&format!("/login/?error={}", urlencode(msg))));
    if let Some(err) = &q.error {
        return fail(err);
    }
    let (Some(code), Some(csrf)) = (&q.code, &q.state) else {
        return fail("missing code");
    };
    let oidc = state.oidc.as_ref().ok_or(ApiError::NotFound)?;
    let pending: Option<(String, String, String)> = sqlx::query_as(
        "DELETE FROM oidc_states WHERE state = $1 AND created_at > now() - interval '15 minutes'
         RETURNING nonce, pkce_verifier, return_to",
    )
    .bind(csrf)
    .fetch_optional(&state.db)
    .await?;
    let Some((nonce, verifier, return_to)) = pending else {
        return fail("login expired, please try again");
    };

    let metadata = oidc.metadata().await?.clone();
    let issuer = metadata.issuer().to_string();
    let client = CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(oidc.config.client_id.clone()),
        oidc.config.client_secret.clone().map(ClientSecret::new),
    )
    .set_redirect_uri(RedirectUrl::new(oidc.redirect_url.clone()).map_err(internal)?);

    let tokens = client
        .exchange_code(AuthorizationCode::new(code.clone()))
        .map_err(internal)?
        .set_pkce_verifier(PkceCodeVerifier::new(verifier))
        .request_async(&oidc.http)
        .await
        .map_err(internal)?;
    let id_token = tokens.id_token().ok_or_else(|| internal("provider returned no id_token"))?;
    let claims = id_token.claims(&client.id_token_verifier(), &Nonce::new(nonce)).map_err(internal)?;

    let subject = claims.subject().to_string();
    let email = claims.email().map(|e| e.to_string());
    let email_verified = claims.email_verified().unwrap_or(false);
    let first = claims.given_name().and_then(|n| n.get(None)).map(|n| n.to_string());
    let last = claims.family_name().and_then(|n| n.get(None)).map(|n| n.to_string());
    let display = claims
        .preferred_username()
        .map(|n| n.to_string())
        .or_else(|| claims.name().and_then(|n| n.get(None)).map(|n| n.to_string()))
        .or_else(|| email.clone())
        .unwrap_or_else(|| "User".into());

    let db = &state.db;
    let mut user_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE oidc_issuer = $1 AND oidc_subject = $2")
            .bind(&issuer)
            .bind(&subject)
            .fetch_optional(db)
            .await?;
    if user_id.is_none() && email_verified {
        if let Some(email) = &email {
            user_id = sqlx::query_scalar(
                "UPDATE users SET oidc_issuer = $1, oidc_subject = $2, updated_at = now()
                 WHERE kind = 'human' AND lower(email) = lower($3) AND oidc_subject IS NULL RETURNING id",
            )
            .bind(&issuer)
            .bind(&subject)
            .bind(email)
            .fetch_optional(db)
            .await?;
        }
    }
    let user_id = match user_id {
        Some(id) => id,
        None => {
            let Some(email) = email else {
                return fail("your account has no email address");
            };
            sqlx::query_scalar(
                "INSERT INTO users (kind, display_name, first_name, last_name, email, oidc_issuer, oidc_subject)
                 VALUES ('human', $1, $2, $3, $4, $5, $6) RETURNING id",
            )
            .bind(display)
            .bind(first)
            .bind(last)
            .bind(email)
            .bind(&issuer)
            .bind(&subject)
            .fetch_one(db)
            .await?
        }
    };

    let token = auth::create_session(db, user_id, state.config.session_days).await?;
    Ok(HttpResponse::Found()
        .cookie(auth::session_cookie(token, &state.config))
        .insert_header((LOCATION, return_to))
        .finish())
}

fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}
