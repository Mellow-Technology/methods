use actix_web::{HttpResponse, web};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    AppState, access,
    activity::{self, Changes},
    auth::AuthUser,
    error::{ApiResult, bad},
    models::{EPIC_COLUMNS, Epic, Label, PROJECT_COLUMNS, Project, double_option, non_empty},
};

pub async fn project_out(db: &sqlx::PgPool, id: Uuid) -> ApiResult<Project> {
    Ok(sqlx::query_as::<_, Project>(&format!("SELECT {PROJECT_COLUMNS} FROM projects p WHERE p.id = $1"))
        .bind(id)
        .fetch_one(db)
        .await?)
}

pub async fn list(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    access::workspace(&state.db, *path, &user).await?;
    let rows = sqlx::query_as::<_, Project>(&format!(
        "SELECT {PROJECT_COLUMNS} FROM projects p WHERE p.workspace_id = $1 ORDER BY lower(p.name)"
    ))
    .bind(*path)
    .fetch_all(&state.db)
    .await?;
    Ok(HttpResponse::Ok().json(rows))
}

fn check_identifier(raw: &str) -> ApiResult<String> {
    let id = raw.trim().to_ascii_uppercase();
    let mut chars = id.chars();
    let ok = (2..=10).contains(&id.len())
        && chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    if ok { Ok(id) } else { Err(bad("project ID must be 2-10 letters or digits, starting with a letter")) }
}

fn check_emoji(e: Option<String>) -> ApiResult<Option<String>> {
    let e = e.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    if e.as_ref().is_some_and(|s| s.chars().count() > 16) {
        return Err(bad("icon must be a single emoji"));
    }
    Ok(e)
}

#[derive(Deserialize)]
pub struct CreateProject {
    name: String,
    identifier: String,
    #[serde(default)]
    description: String,
    icon_emoji: Option<String>,
    icon_file_id: Option<Uuid>,
    cover_file_id: Option<Uuid>,
}

pub async fn create(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    body: web::Json<CreateProject>,
) -> ApiResult<HttpResponse> {
    access::workspace(&state.db, *path, &user).await?;
    let b = body.into_inner();
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO projects (workspace_id, name, identifier, description, icon_emoji, icon_file_id, cover_file_id, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
    )
    .bind(*path)
    .bind(non_empty("name", &b.name)?)
    .bind(check_identifier(&b.identifier)?)
    .bind(b.description.trim())
    .bind(check_emoji(b.icon_emoji)?)
    .bind(b.icon_file_id)
    .bind(b.cover_file_id)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    let snap = project_fields(&mut tx, id).await?;
    let target = activity::project_target(&mut tx, id, "project", id, snap.label()).await?;
    let mut c = Changes::new();
    c.value("name", &snap.name).value("identifier", &snap.identifier);
    activity::record(&mut tx, &user, "created", &target, c.into_value()).await?;
    tx.commit().await?;
    Ok(HttpResponse::Created().json(project_out(&state.db, id).await?))
}

pub async fn get(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    access::project(&state.db, *path, &user).await?;
    Ok(HttpResponse::Ok().json(project_out(&state.db, *path).await?))
}

#[derive(Deserialize)]
pub struct UpdateProject {
    name: Option<String>,
    identifier: Option<String>,
    description: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    icon_emoji: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    icon_file_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    cover_file_id: Option<Option<Uuid>>,
}

pub async fn update(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    body: web::Json<UpdateProject>,
) -> ApiResult<HttpResponse> {
    access::project(&state.db, *path, &user).await?;
    let b = body.into_inner();
    let mut q = sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE projects SET updated_at = now()");
    if let Some(name) = &b.name {
        q.push(", name = ").push_bind(non_empty("name", name)?);
    }
    if let Some(ident) = &b.identifier {
        q.push(", identifier = ").push_bind(check_identifier(ident)?);
    }
    if let Some(desc) = &b.description {
        q.push(", description = ").push_bind(desc.trim().to_string());
    }
    if let Some(emoji) = b.icon_emoji {
        q.push(", icon_emoji = ").push_bind(check_emoji(emoji)?);
    }
    if let Some(icon) = b.icon_file_id {
        q.push(", icon_file_id = ").push_bind(icon);
    }
    if let Some(cover) = b.cover_file_id {
        q.push(", cover_file_id = ").push_bind(cover);
    }
    q.push(" WHERE id = ").push_bind(*path);
    let mut tx = state.db.begin().await?;
    let before = project_fields(&mut tx, *path).await?;
    q.build().execute(&mut *tx).await?;
    let after = project_fields(&mut tx, *path).await?;
    let mut c = Changes::new();
    c.set("name", &before.name, &after.name)
        .set("identifier", &before.identifier, &after.identifier)
        .set("description", &before.description, &after.description)
        .set("icon", (&before.icon_emoji, before.icon_file_id), (&after.icon_emoji, after.icon_file_id))
        .set("cover", before.cover_file_id, after.cover_file_id);
    if !c.is_empty() {
        let target = activity::project_target(&mut tx, *path, "project", *path, after.label()).await?;
        activity::record(&mut tx, &user, "updated", &target, c.into_value()).await?;
    }
    tx.commit().await?;
    Ok(HttpResponse::Ok().json(project_out(&state.db, *path).await?))
}

/// Deleting a project is permanent (its items go with it), so only the
/// history entry remains.
pub async fn delete(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    access::project(&state.db, *path, &user).await?.require_admin()?;
    let mut tx = state.db.begin().await?;
    let snap = project_fields(&mut tx, *path).await?;
    let target = activity::project_target(&mut tx, *path, "project", *path, snap.label()).await?;
    activity::record(&mut tx, &user, "deleted", &target, serde_json::json!({})).await?;
    sqlx::query("DELETE FROM projects WHERE id = $1").bind(*path).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}

#[derive(sqlx::FromRow)]
struct ProjectFields {
    name: String,
    identifier: String,
    description: String,
    icon_emoji: Option<String>,
    icon_file_id: Option<Uuid>,
    cover_file_id: Option<Uuid>,
}

impl ProjectFields {
    fn label(&self) -> String {
        format!("{} ({})", self.name, self.identifier)
    }
}

async fn project_fields(conn: &mut sqlx::PgConnection, id: Uuid) -> ApiResult<ProjectFields> {
    Ok(sqlx::query_as::<_, ProjectFields>(
        "SELECT name, identifier, description, icon_emoji, icon_file_id, cover_file_id FROM projects WHERE id = $1",
    )
    .bind(id)
    .fetch_one(conn)
    .await?)
}

// ---- Labels ----------------------------------------------------------------

pub async fn list_labels_for(db: &sqlx::PgPool, project_id: Uuid) -> ApiResult<Vec<Label>> {
    Ok(sqlx::query_as::<_, Label>(
        "SELECT id, project_id, name, color FROM labels WHERE project_id = $1 ORDER BY lower(name)",
    )
    .bind(project_id)
    .fetch_all(db)
    .await?)
}

pub async fn list_labels(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    access::project(&state.db, *path, &user).await?;
    Ok(HttpResponse::Ok().json(list_labels_for(&state.db, *path).await?))
}

#[derive(Deserialize)]
pub struct LabelBody {
    name: Option<String>,
    color: Option<String>,
}

fn check_color(c: &str) -> ApiResult<String> {
    let c = c.trim();
    let ok = c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit());
    if ok { Ok(c.to_lowercase()) } else { Err(bad("colour must look like #a1b2c3")) }
}

pub async fn create_label(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    body: web::Json<LabelBody>,
) -> ApiResult<HttpResponse> {
    access::project(&state.db, *path, &user).await?;
    let mut tx = state.db.begin().await?;
    let label = sqlx::query_as::<_, Label>(
        "INSERT INTO labels (project_id, name, color) VALUES ($1, $2, $3) RETURNING id, project_id, name, color",
    )
    .bind(*path)
    .bind(non_empty("name", body.name.as_deref().unwrap_or(""))?)
    .bind(check_color(body.color.as_deref().unwrap_or("#64748b"))?)
    .fetch_one(&mut *tx)
    .await?;
    let target = activity::project_target(&mut tx, *path, "label", label.id, label.name.clone()).await?;
    let mut c = Changes::new();
    c.value("name", &label.name).value("color", &label.color);
    activity::record(&mut tx, &user, "created", &target, c.into_value()).await?;
    tx.commit().await?;
    Ok(HttpResponse::Created().json(label))
}

pub async fn update_label(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    body: web::Json<LabelBody>,
) -> ApiResult<HttpResponse> {
    access::via(&state.db, "labels", *path, &user).await?;
    let name = body.name.as_deref().map(|n| non_empty("name", n)).transpose()?;
    let color = body.color.as_deref().map(check_color).transpose()?;
    let mut tx = state.db.begin().await?;
    let old = sqlx::query_as::<_, Label>("SELECT id, project_id, name, color FROM labels WHERE id = $1 FOR UPDATE")
        .bind(*path)
        .fetch_one(&mut *tx)
        .await?;
    let label = sqlx::query_as::<_, Label>(
        "UPDATE labels SET name = COALESCE($2, name), color = COALESCE($3, color) WHERE id = $1
         RETURNING id, project_id, name, color",
    )
    .bind(*path)
    .bind(name)
    .bind(color)
    .fetch_one(&mut *tx)
    .await?;
    let mut c = Changes::new();
    c.set("name", &old.name, &label.name).set("color", &old.color, &label.color);
    if !c.is_empty() {
        let target = activity::project_target(&mut tx, label.project_id, "label", label.id, label.name.clone()).await?;
        activity::record(&mut tx, &user, "updated", &target, c.into_value()).await?;
    }
    tx.commit().await?;
    Ok(HttpResponse::Ok().json(label))
}

pub async fn delete_label(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    access::via(&state.db, "labels", *path, &user).await?;
    let mut tx = state.db.begin().await?;
    let old = sqlx::query_as::<_, Label>("DELETE FROM labels WHERE id = $1 RETURNING id, project_id, name, color")
        .bind(*path)
        .fetch_one(&mut *tx)
        .await?;
    let target = activity::project_target(&mut tx, old.project_id, "label", old.id, old.name.clone()).await?;
    activity::record(&mut tx, &user, "deleted", &target, Changes::new().value("color", &old.color).take()).await?;
    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}

// ---- Epics -----------------------------------------------------------------

pub async fn list_epics_for(db: &sqlx::PgPool, project_id: Uuid) -> ApiResult<Vec<Epic>> {
    Ok(sqlx::query_as::<_, Epic>(&format!(
        "SELECT {EPIC_COLUMNS} FROM epics e WHERE e.project_id = $1 ORDER BY e.created_at"
    ))
    .bind(project_id)
    .fetch_all(db)
    .await?)
}

pub async fn list_epics(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    access::project(&state.db, *path, &user).await?;
    Ok(HttpResponse::Ok().json(list_epics_for(&state.db, *path).await?))
}

#[derive(Deserialize)]
pub struct EpicBody {
    pub name: Option<String>,
    pub description: Option<String>,
    pub color: Option<String>,
}

pub async fn create_epic_for(db: &sqlx::PgPool, user: &AuthUser, project_id: Uuid, b: &EpicBody) -> ApiResult<Epic> {
    access::project(db, project_id, user).await?;
    let name = non_empty("name", b.name.as_deref().unwrap_or(""))?;
    let mut tx = db.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO epics (project_id, name, description, color, created_by) VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(project_id)
    .bind(&name)
    .bind(b.description.as_deref().unwrap_or("").trim())
    .bind(check_color(b.color.as_deref().unwrap_or("#6366f1"))?)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    let target = activity::project_target(&mut tx, project_id, "epic", id, name.clone()).await?;
    activity::record(&mut tx, user, "created", &target, Changes::new().value("name", &name).take()).await?;
    tx.commit().await?;
    Ok(sqlx::query_as::<_, Epic>(&format!("SELECT {EPIC_COLUMNS} FROM epics e WHERE e.id = $1"))
        .bind(id)
        .fetch_one(db)
        .await?)
}

pub async fn create_epic(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    body: web::Json<EpicBody>,
) -> ApiResult<HttpResponse> {
    Ok(HttpResponse::Created().json(create_epic_for(&state.db, &user, *path, &body).await?))
}

pub async fn update_epic(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    body: web::Json<EpicBody>,
) -> ApiResult<HttpResponse> {
    access::via(&state.db, "epics", *path, &user).await?;
    let name = body.name.as_deref().map(|n| non_empty("name", n)).transpose()?;
    let color = body.color.as_deref().map(check_color).transpose()?;
    let mut tx = state.db.begin().await?;
    const FIELDS: &str = "SELECT project_id, name, description, color FROM epics WHERE id = $1";
    let before: (Uuid, String, String, String) =
        sqlx::query_as(&format!("{FIELDS} FOR UPDATE")).bind(*path).fetch_one(&mut *tx).await?;
    sqlx::query(
        "UPDATE epics SET name = COALESCE($2, name), description = COALESCE($3, description),
                          color = COALESCE($4, color), updated_at = now() WHERE id = $1",
    )
    .bind(*path)
    .bind(name)
    .bind(body.description.as_deref().map(str::trim))
    .bind(color)
    .execute(&mut *tx)
    .await?;
    let after: (Uuid, String, String, String) = sqlx::query_as(FIELDS).bind(*path).fetch_one(&mut *tx).await?;
    let mut c = Changes::new();
    c.set("name", &before.1, &after.1).set("description", &before.2, &after.2).set("color", &before.3, &after.3);
    if !c.is_empty() {
        let target = activity::project_target(&mut tx, after.0, "epic", *path, after.1.clone()).await?;
        activity::record(&mut tx, &user, "updated", &target, c.into_value()).await?;
    }
    tx.commit().await?;
    let epic = sqlx::query_as::<_, Epic>(&format!("SELECT {EPIC_COLUMNS} FROM epics e WHERE e.id = $1"))
        .bind(*path)
        .fetch_one(&state.db)
        .await?;
    Ok(HttpResponse::Ok().json(epic))
}

pub async fn delete_epic(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    access::via(&state.db, "epics", *path, &user).await?;
    let mut tx = state.db.begin().await?;
    let (project_id, name): (Uuid, String) =
        sqlx::query_as("DELETE FROM epics WHERE id = $1 RETURNING project_id, name")
            .bind(*path)
            .fetch_one(&mut *tx)
            .await?;
    let target = activity::project_target(&mut tx, project_id, "epic", *path, name).await?;
    activity::record(&mut tx, &user, "deleted", &target, serde_json::json!({})).await?;
    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}
