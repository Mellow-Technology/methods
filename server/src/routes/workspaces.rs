use actix_web::{HttpResponse, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{
    AppState, access,
    activity::{self, Changes, workspace_target},
    auth::{self, AuthUser},
    error::{ApiError, ApiResult, bad},
    models::{Member, Workspace, non_empty},
};

const WORKSPACE_SELECT: &str = "SELECT w.id, w.name, w.slug, m.role,
    (SELECT count(*) FROM projects p WHERE p.workspace_id = w.id) AS project_count, w.created_at
    FROM workspaces w JOIN workspace_members m ON m.workspace_id = w.id AND m.user_id = $1";

pub async fn list(state: web::Data<AppState>, user: AuthUser) -> ApiResult<HttpResponse> {
    let rows = sqlx::query_as::<_, Workspace>(&format!("{WORKSPACE_SELECT} ORDER BY lower(w.name)"))
        .bind(user.id)
        .fetch_all(&state.db)
        .await?;
    Ok(HttpResponse::Ok().json(rows))
}

pub async fn get(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    let row = sqlx::query_as::<_, Workspace>(&format!("{WORKSPACE_SELECT} WHERE w.id = $2"))
        .bind(user.id)
        .bind(*path)
        .fetch_optional(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(HttpResponse::Ok().json(row))
}

fn slugify(name: &str) -> String {
    let mut slug = String::new();
    for c in name.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-').chars().take(40).collect::<String>();
    if slug.is_empty() { "workspace".into() } else { slug }
}

#[derive(Deserialize)]
pub struct CreateWorkspace {
    name: String,
}

pub async fn create(state: web::Data<AppState>, user: AuthUser, body: web::Json<CreateWorkspace>) -> ApiResult<HttpResponse> {
    if user.is_bot() {
        return Err(ApiError::Forbidden);
    }
    let name = non_empty("name", &body.name)?;
    let base = slugify(&name);
    let mut tx = state.db.begin().await?;
    let mut slug = base.clone();
    for _ in 0..5 {
        let taken: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM workspaces WHERE slug = $1)")
            .bind(&slug)
            .fetch_one(&mut *tx)
            .await?;
        if !taken {
            break;
        }
        slug = format!("{base}-{}", &auth::random_token(4).to_lowercase()[..4]);
    }
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO workspaces (name, slug, created_by) VALUES ($1, $2, $3) RETURNING id")
            .bind(&name)
            .bind(&slug)
            .bind(user.id)
            .fetch_one(&mut *tx)
            .await?;
    sqlx::query("INSERT INTO workspace_members (workspace_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(id)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    let target = workspace_target(id, "workspace", id, name.clone());
    activity::record(&mut tx, &user, "created", &target, Changes::new().value("name", &name).take()).await?;
    tx.commit().await?;
    get(state, user, web::Path::from(id)).await
}

pub async fn update(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    body: web::Json<CreateWorkspace>,
) -> ApiResult<HttpResponse> {
    access::workspace(&state.db, *path, &user).await?.require_admin()?;
    let name = non_empty("name", &body.name)?;
    let mut tx = state.db.begin().await?;
    let old: String = sqlx::query_scalar("SELECT name FROM workspaces WHERE id = $1 FOR UPDATE")
        .bind(*path)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("UPDATE workspaces SET name = $1, updated_at = now() WHERE id = $2")
        .bind(&name)
        .bind(*path)
        .execute(&mut *tx)
        .await?;
    if old != name {
        let target = workspace_target(*path, "workspace", *path, name.clone());
        let mut c = Changes::new();
        c.set("name", &old, &name);
        activity::record(&mut tx, &user, "updated", &target, c.into_value()).await?;
    }
    tx.commit().await?;
    get(state, user, path).await
}

pub async fn members(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    access::workspace(&state.db, *path, &user).await?;
    let rows = sqlx::query_as::<_, Member>(
        "SELECT u.id, u.kind, u.display_name, u.email, '/api/files/' || u.avatar_file_id::text AS avatar_url, m.role
         FROM workspace_members m JOIN users u ON u.id = m.user_id
         WHERE m.workspace_id = $1 ORDER BY u.kind DESC, lower(u.display_name)",
    )
    .bind(*path)
    .fetch_all(&state.db)
    .await?;
    Ok(HttpResponse::Ok().json(rows))
}

#[derive(Deserialize)]
pub struct AddMember {
    email: String,
    role: Option<String>,
}

fn check_role(role: &str) -> ApiResult<()> {
    if ["admin", "member"].contains(&role) { Ok(()) } else { Err(bad("role must be admin or member")) }
}

pub async fn add_member(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    body: web::Json<AddMember>,
) -> ApiResult<HttpResponse> {
    access::workspace(&state.db, *path, &user).await?.require_admin()?;
    let role = body.role.clone().unwrap_or_else(|| "member".into());
    check_role(&role)?;
    let target: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE kind = 'human' AND lower(email) = lower($1)")
            .bind(body.email.trim())
            .fetch_optional(&state.db)
            .await?;
    let target = target.ok_or_else(|| bad("no one has signed up with that email yet"))?;
    let mut tx = state.db.begin().await?;
    let added = sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, role) VALUES ($1, $2, $3)
         ON CONFLICT (workspace_id, user_id) DO NOTHING",
    )
    .bind(*path)
    .bind(target)
    .bind(&role)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if added > 0 {
        let t = workspace_target(*path, "member", target, user_name(&mut tx, target).await?);
        activity::record(&mut tx, &user, "created", &t, Changes::new().value("role", &role).take()).await?;
    }
    tx.commit().await?;
    members(state, user, path).await
}

#[derive(Deserialize)]
pub struct UpdateMember {
    role: String,
}

pub async fn update_member(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<(Uuid, Uuid)>,
    body: web::Json<UpdateMember>,
) -> ApiResult<HttpResponse> {
    let (ws, member) = path.into_inner();
    access::workspace(&state.db, ws, &user).await?.require_admin()?;
    check_role(&body.role)?;
    let mut tx = state.db.begin().await?;
    let old: Option<String> = sqlx::query_scalar(
        "SELECT role FROM workspace_members WHERE workspace_id = $1 AND user_id = $2 AND role <> 'owner' FOR UPDATE",
    )
    .bind(ws)
    .bind(member)
    .fetch_optional(&mut *tx)
    .await?;
    let old = old.ok_or_else(|| bad("that member's role can't be changed"))?;
    sqlx::query("UPDATE workspace_members SET role = $1 WHERE workspace_id = $2 AND user_id = $3")
        .bind(&body.role)
        .bind(ws)
        .bind(member)
        .execute(&mut *tx)
        .await?;
    if old != body.role {
        let t = workspace_target(ws, "member", member, user_name(&mut tx, member).await?);
        let mut c = Changes::new();
        c.set("role", &old, &body.role);
        activity::record(&mut tx, &user, "updated", &t, c.into_value()).await?;
    }
    tx.commit().await?;
    members(state, user, web::Path::from(ws)).await
}

pub async fn remove_member(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<(Uuid, Uuid)>,
) -> ApiResult<HttpResponse> {
    let (ws, member) = path.into_inner();
    let access = access::workspace(&state.db, ws, &user).await?;
    if member != user.id {
        access.require_admin()?;
    }
    let mut tx = state.db.begin().await?;
    let n = sqlx::query("DELETE FROM workspace_members WHERE workspace_id = $1 AND user_id = $2 AND role <> 'owner'")
        .bind(ws)
        .bind(member)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(bad("the workspace owner can't be removed"));
    }
    let t = workspace_target(ws, "member", member, user_name(&mut tx, member).await?);
    activity::record(&mut tx, &user, "deleted", &t, serde_json::json!({})).await?;
    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}

async fn user_name(conn: &mut sqlx::PgConnection, id: Uuid) -> ApiResult<String> {
    Ok(sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1").bind(id).fetch_one(conn).await?)
}

// ---- Bots ------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
pub struct Bot {
    id: Uuid,
    display_name: String,
    avatar_url: Option<String>,
    workspace_id: Uuid,
    active_tokens: i64,
    last_used_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

const BOT_SELECT: &str = "SELECT u.id, u.display_name, '/api/files/' || u.avatar_file_id::text AS avatar_url,
    u.bot_workspace_id AS workspace_id,
    (SELECT count(*) FROM bot_tokens t WHERE t.bot_id = u.id AND t.revoked_at IS NULL) AS active_tokens,
    (SELECT max(t.last_used_at) FROM bot_tokens t WHERE t.bot_id = u.id) AS last_used_at,
    u.created_at
    FROM users u WHERE u.kind = 'bot'";

pub async fn list_bots(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    access::workspace(&state.db, *path, &user).await?;
    let rows = sqlx::query_as::<_, Bot>(&format!("{BOT_SELECT} AND u.bot_workspace_id = $1 ORDER BY lower(u.display_name)"))
        .bind(*path)
        .fetch_all(&state.db)
        .await?;
    Ok(HttpResponse::Ok().json(rows))
}

#[derive(Deserialize)]
pub struct CreateBot {
    name: String,
}

pub async fn create_bot(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    body: web::Json<CreateBot>,
) -> ApiResult<HttpResponse> {
    let ws = *path;
    access::workspace(&state.db, ws, &user).await?.require_admin()?;
    let name = non_empty("name", &body.name)?;
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (kind, display_name, bot_workspace_id, created_by) VALUES ('bot', $1, $2, $3) RETURNING id",
    )
    .bind(&name)
    .bind(ws)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO workspace_members (workspace_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(ws)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    activity::record(&mut tx, &user, "created", &workspace_target(ws, "bot", id, name), serde_json::json!({})).await?;
    tx.commit().await?;
    let bot = sqlx::query_as::<_, Bot>(&format!("{BOT_SELECT} AND u.id = $1")).bind(id).fetch_one(&state.db).await?;
    Ok(HttpResponse::Created().json(bot))
}

/// Admin access to the workspace a bot belongs to.
/// Returns the workspace and the bot's name.
async fn bot_admin(state: &AppState, user: &AuthUser, bot: Uuid) -> ApiResult<(Uuid, String)> {
    let row: Option<(Option<Uuid>, String)> =
        sqlx::query_as("SELECT bot_workspace_id, display_name FROM users WHERE id = $1 AND kind = 'bot'")
            .bind(bot)
            .fetch_optional(&state.db)
            .await?;
    let (ws, name) = row.ok_or(ApiError::NotFound)?;
    let ws = ws.ok_or(ApiError::NotFound)?;
    access::workspace(&state.db, ws, user).await?.require_admin()?;
    Ok((ws, name))
}

#[derive(Deserialize)]
pub struct UpdateBot {
    name: Option<String>,
    avatar_file_id: Option<Uuid>,
}

pub async fn update_bot(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    body: web::Json<UpdateBot>,
) -> ApiResult<HttpResponse> {
    let (ws, old_name) = bot_admin(&state, &user, *path).await?;
    let mut tx = state.db.begin().await?;
    let mut c = Changes::new();
    let mut name = old_name.clone();
    if let Some(n) = &body.name {
        name = non_empty("name", n)?;
        sqlx::query("UPDATE users SET display_name = $1, updated_at = now() WHERE id = $2")
            .bind(&name)
            .bind(*path)
            .execute(&mut *tx)
            .await?;
        c.set("name", &old_name, &name);
    }
    if let Some(avatar) = body.avatar_file_id {
        sqlx::query("UPDATE users SET avatar_file_id = $1 WHERE id = $2").bind(avatar).bind(*path).execute(&mut *tx).await?;
        c.set("avatar", "previous", "new");
    }
    if !c.is_empty() {
        activity::record(&mut tx, &user, "updated", &workspace_target(ws, "bot", *path, name), c.into_value()).await?;
    }
    tx.commit().await?;
    let bot = sqlx::query_as::<_, Bot>(&format!("{BOT_SELECT} AND u.id = $1")).bind(*path).fetch_one(&state.db).await?;
    Ok(HttpResponse::Ok().json(bot))
}

pub async fn delete_bot(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    let (ws, name) = bot_admin(&state, &user, *path).await?;
    let mut tx = state.db.begin().await?;
    activity::record(&mut tx, &user, "deleted", &workspace_target(ws, "bot", *path, name), serde_json::json!({})).await?;
    sqlx::query("DELETE FROM users WHERE id = $1 AND kind = 'bot'").bind(*path).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Serialize, sqlx::FromRow)]
pub struct BotToken {
    id: Uuid,
    name: String,
    token_prefix: String,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
}

pub async fn list_tokens(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    bot_admin(&state, &user, *path).await?;
    let rows = sqlx::query_as::<_, BotToken>(
        "SELECT id, name, token_prefix, created_at, last_used_at, revoked_at FROM bot_tokens
         WHERE bot_id = $1 ORDER BY revoked_at IS NOT NULL, created_at DESC",
    )
    .bind(*path)
    .fetch_all(&state.db)
    .await?;
    Ok(HttpResponse::Ok().json(rows))
}

#[derive(Deserialize)]
pub struct CreateToken {
    name: Option<String>,
}

pub async fn create_token(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    body: web::Json<CreateToken>,
) -> ApiResult<HttpResponse> {
    let (ws, bot_name) = bot_admin(&state, &user, *path).await?;
    let (token, prefix, hash) = auth::new_bot_token();
    let name = body.name.as_deref().map(str::trim).filter(|n| !n.is_empty()).unwrap_or("MCP token");
    let mut tx = state.db.begin().await?;
    let row = sqlx::query_as::<_, BotToken>(
        "INSERT INTO bot_tokens (bot_id, name, token_prefix, token_hash, created_by) VALUES ($1, $2, $3, $4, $5)
         RETURNING id, name, token_prefix, created_at, last_used_at, revoked_at",
    )
    .bind(*path)
    .bind(name)
    .bind(prefix)
    .bind(hash)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    let t = workspace_target(ws, "bot_token", row.id, format!("{bot_name}: {name}"));
    activity::record(&mut tx, &user, "created", &t, Changes::new().value("prefix", &row.token_prefix).take()).await?;
    tx.commit().await?;
    Ok(HttpResponse::Created().json(json!({
        "token": token,
        "details": row,
        "mcp_url": format!("{}/mcp", state.config.public_url),
    })))
}

pub async fn revoke_token(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    let bot: Option<Uuid> = sqlx::query_scalar("SELECT bot_id FROM bot_tokens WHERE id = $1")
        .bind(*path)
        .fetch_optional(&state.db)
        .await?;
    let (ws, bot_name) = bot_admin(&state, &user, bot.ok_or(ApiError::NotFound)?).await?;
    let mut tx = state.db.begin().await?;
    let revoked: Option<String> = sqlx::query_scalar(
        "UPDATE bot_tokens SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL RETURNING name",
    )
    .bind(*path)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(name) = revoked {
        let t = workspace_target(ws, "bot_token", *path, format!("{bot_name}: {name}"));
        activity::record(&mut tx, &user, "revoked", &t, serde_json::json!({})).await?;
    }
    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}
