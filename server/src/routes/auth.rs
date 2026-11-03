use actix_web::{HttpRequest, HttpResponse, web};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::{
    AppState,
    auth::{self, AuthUser},
    error::{ApiError, ApiResult, bad},
    models::{USER_COLUMNS, UserOut, double_option, non_empty},
};

pub async fn config(state: web::Data<AppState>) -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "password_login": state.config.password_login,
        "allow_registration": state.config.password_login && state.config.allow_registration,
        "oidc": state.config.oidc.as_ref().map(|o| json!({ "name": o.display_name })),
    }))
}

#[derive(Deserialize)]
pub struct RegisterBody {
    email: String,
    password: String,
    first_name: Option<String>,
    last_name: Option<String>,
    display_name: Option<String>,
}

fn check_email(email: &str) -> ApiResult<String> {
    let e = email.trim().to_string();
    let valid = e.len() <= 254 && e.split_once('@').is_some_and(|(a, b)| !a.is_empty() && b.contains('.'));
    if valid { Ok(e) } else { Err(bad("enter a valid email address")) }
}

pub async fn user_out(db: &sqlx::PgPool, id: Uuid) -> ApiResult<UserOut> {
    Ok(sqlx::query_as::<_, UserOut>(&format!("SELECT {USER_COLUMNS} FROM users u WHERE u.id = $1"))
        .bind(id)
        .fetch_one(db)
        .await?)
}

async fn login_response(state: &AppState, user_id: Uuid) -> ApiResult<HttpResponse> {
    let token = auth::create_session(&state.db, user_id, state.config.session_days).await?;
    let user = user_out(&state.db, user_id).await?;
    Ok(HttpResponse::Ok().cookie(auth::session_cookie(token, &state.config)).json(user))
}

pub async fn register(state: web::Data<AppState>, body: web::Json<RegisterBody>) -> ApiResult<HttpResponse> {
    if !(state.config.password_login && state.config.allow_registration) {
        return Err(ApiError::Forbidden);
    }
    let email = check_email(&body.email)?;
    if body.password.chars().count() < 8 {
        return Err(bad("password must be at least 8 characters"));
    }
    let first = body.first_name.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let last = body.last_name.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let display = match body.display_name.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(d) => d.to_string(),
        None => match (first, last) {
            (Some(f), Some(l)) => format!("{f} {l}"),
            (Some(f), None) => f.to_string(),
            _ => email.split('@').next().unwrap_or("user").to_string(),
        },
    };
    let hash = auth::hash_password(&body.password)?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (kind, display_name, first_name, last_name, email, password_hash)
         VALUES ('human', $1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(display)
    .bind(first)
    .bind(last)
    .bind(&email)
    .bind(hash)
    .fetch_one(&state.db)
    .await?;
    login_response(&state, id).await
}

#[derive(Deserialize)]
pub struct LoginBody {
    email: String,
    password: String,
}

pub async fn login(state: web::Data<AppState>, body: web::Json<LoginBody>) -> ApiResult<HttpResponse> {
    if !state.config.password_login {
        return Err(ApiError::Forbidden);
    }
    let row: Option<(Uuid, Option<String>)> = sqlx::query_as(
        "SELECT id, password_hash FROM users WHERE kind = 'human' AND lower(email) = lower($1)",
    )
    .bind(body.email.trim())
    .fetch_optional(&state.db)
    .await?;
    match row {
        Some((id, Some(hash))) if auth::verify_password(&body.password, &hash) => login_response(&state, id).await,
        _ => Err(bad("incorrect email or password")),
    }
}

pub async fn logout(state: web::Data<AppState>, req: HttpRequest) -> ApiResult<HttpResponse> {
    if let Some(token) = auth::request_token(&req) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
            .bind(auth::hash_token(&token))
            .execute(&state.db)
            .await?;
    }
    Ok(HttpResponse::NoContent().cookie(auth::clear_session_cookie(&state.config)).finish())
}

pub async fn me(state: web::Data<AppState>, user: AuthUser) -> ApiResult<HttpResponse> {
    Ok(HttpResponse::Ok().json(user_out(&state.db, user.id).await?))
}

#[derive(Deserialize)]
pub struct UpdateMe {
    display_name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    first_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    last_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    avatar_file_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    cover_file_id: Option<Option<Uuid>>,
    current_password: Option<String>,
    new_password: Option<String>,
}

pub async fn update_me(state: web::Data<AppState>, user: AuthUser, body: web::Json<UpdateMe>) -> ApiResult<HttpResponse> {
    let db = &state.db;
    let b = body.into_inner();
    let mut q = sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE users SET updated_at = now()");
    if let Some(d) = &b.display_name {
        q.push(", display_name = ").push_bind(non_empty("display name", d)?);
    }
    let tidy = |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    if let Some(f) = b.first_name {
        q.push(", first_name = ").push_bind(tidy(f));
    }
    if let Some(l) = b.last_name {
        q.push(", last_name = ").push_bind(tidy(l));
    }
    if let Some(a) = b.avatar_file_id {
        q.push(", avatar_file_id = ").push_bind(a);
    }
    if let Some(c) = b.cover_file_id {
        q.push(", cover_file_id = ").push_bind(c);
    }
    if let Some(new) = &b.new_password {
        if new.chars().count() < 8 {
            return Err(bad("password must be at least 8 characters"));
        }
        let existing: Option<String> = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
            .bind(user.id)
            .fetch_one(db)
            .await?;
        if let Some(hash) = existing {
            let current = b.current_password.as_deref().unwrap_or("");
            if !auth::verify_password(current, &hash) {
                return Err(bad("current password is incorrect"));
            }
        }
        q.push(", password_hash = ").push_bind(auth::hash_password(new)?);
    }
    q.push(" WHERE id = ").push_bind(user.id);
    q.build().execute(db).await?;
    Ok(HttpResponse::Ok().json(user_out(db, user.id).await?))
}
