use actix_multipart::Multipart;
use actix_web::{HttpResponse, web};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    AppState, access,
    activity::{self, Changes, Page, Scope, Target},
    auth::AuthUser,
    error::{ApiError, ApiResult, bad},
    models::{COMMENT_SELECT, Comment},
    routes::files,
    service::{self, CreateWorkItem, UpdateWorkItem, WorkItemFilter},
};

pub async fn list(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    filter: web::Query<WorkItemFilter>,
) -> ApiResult<HttpResponse> {
    access::project(&state.db, *path, &user).await?;
    Ok(HttpResponse::Ok().json(service::list(&state.db, *path, &filter).await?))
}

pub async fn create(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    body: web::Json<CreateWorkItem>,
) -> ApiResult<HttpResponse> {
    Ok(HttpResponse::Created().json(service::create(&state.db, &user, *path, body.into_inner()).await?))
}

/// Accepts an id or a key such as `MTH-12`.
pub async fn get(state: web::Data<AppState>, user: AuthUser, path: web::Path<String>) -> ApiResult<HttpResponse> {
    let id = service::resolve(&state.db, &user, &path).await?;
    Ok(HttpResponse::Ok().json(service::detail(&state.db, id, &user).await?))
}

pub async fn update(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    body: web::Json<UpdateWorkItem>,
) -> ApiResult<HttpResponse> {
    Ok(HttpResponse::Ok().json(service::update(&state.db, &user, *path, body.into_inner()).await?))
}

pub async fn delete(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    service::delete(&state.db, &user, *path).await?;
    Ok(HttpResponse::NoContent().finish())
}

// ---- Comments --------------------------------------------------------------

pub async fn list_comments(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    Ok(HttpResponse::Ok().json(service::list_comments(&state.db, &user, *path).await?))
}

#[derive(Deserialize)]
pub struct CommentBody {
    /// A rich text document, or Markdown.
    body: serde_json::Value,
}

pub async fn add_comment(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    body: web::Json<CommentBody>,
) -> ApiResult<HttpResponse> {
    Ok(HttpResponse::Created().json(service::add_comment(&state.db, &user, *path, &body.body).await?))
}

async fn comment_access(state: &AppState, user: &AuthUser, id: Uuid) -> ApiResult<(bool, access::Access)> {
    let row: Option<(Uuid, Option<Uuid>)> =
        sqlx::query_as("SELECT work_item_id, author_id FROM comments WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?;
    let (item, author) = row.ok_or(ApiError::NotFound)?;
    let (_, access) = access::work_item(&state.db, item, user).await?;
    Ok((author == Some(user.id), access))
}

pub async fn update_comment(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    body: web::Json<CommentBody>,
) -> ApiResult<HttpResponse> {
    let (is_author, _) = comment_access(&state, &user, *path).await?;
    if !is_author {
        return Err(ApiError::Forbidden);
    }
    let new_body = service::comment_body(&body.body)?;
    let mut tx = state.db.begin().await?;
    let (old_body, item): (serde_json::Value, Uuid) =
        sqlx::query_as("SELECT body, work_item_id FROM comments WHERE id = $1 FOR UPDATE")
            .bind(*path)
            .fetch_one(&mut *tx)
            .await?;
    if old_body != new_body {
        sqlx::query("UPDATE comments SET body = $1, updated_at = now() WHERE id = $2")
            .bind(&new_body)
            .bind(*path)
            .execute(&mut *tx)
            .await?;
        let target = Target::on_item(&activity::item_target(&mut tx, item).await?, "comment", *path);
        let mut c = Changes::new();
        c.set("body", &old_body, &new_body);
        activity::record(&mut tx, &user, "updated", &target, c.into_value()).await?;
    }
    tx.commit().await?;
    let c = sqlx::query_as::<_, Comment>(&format!("{COMMENT_SELECT} WHERE c.id = $1"))
        .bind(*path)
        .fetch_one(&state.db)
        .await?;
    Ok(HttpResponse::Ok().json(c))
}

pub async fn delete_comment(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    let (is_author, access) = comment_access(&state, &user, *path).await?;
    if !is_author && !access.is_admin() {
        return Err(ApiError::Forbidden);
    }
    let mut tx = state.db.begin().await?;
    let item: Uuid = sqlx::query_scalar(
        "UPDATE comments SET deleted_at = now(), deleted_by = $2 WHERE id = $1 RETURNING work_item_id",
    )
    .bind(*path)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    let target = Target::on_item(&activity::item_target(&mut tx, item).await?, "comment", *path);
    activity::record(&mut tx, &user, "deleted", &target, serde_json::json!({})).await?;
    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}

// ---- Relations, links, reactions, attachments -----------------------------

#[derive(Deserialize)]
pub struct RelationBody {
    target_id: String,
    kind: String,
}

pub async fn add_relation(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    body: web::Json<RelationBody>,
) -> ApiResult<HttpResponse> {
    let target = service::resolve(&state.db, &user, &body.target_id).await?;
    service::add_relation(&state.db, &user, *path, target, &body.kind).await?;
    Ok(HttpResponse::Ok().json(service::detail(&state.db, *path, &user).await?))
}

pub async fn delete_relation(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    let row: Option<(Uuid, Uuid, String)> =
        sqlx::query_as("SELECT source_id, target_id, kind FROM work_item_relations WHERE id = $1")
            .bind(*path)
            .fetch_optional(&state.db)
            .await?;
    let (source, target, kind) = row.ok_or(ApiError::NotFound)?;
    access::work_item(&state.db, source, &user).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM work_item_relations WHERE id = $1").bind(*path).execute(&mut *tx).await?;
    let other = activity::item_target(&mut tx, target).await?;
    let mut t = Target::on_item(&activity::item_target(&mut tx, source).await?, "relation", *path);
    t.related_item_id = Some(target);
    let mut c = Changes::new();
    c.value("kind", &kind).value("item", &other.label);
    activity::record(&mut tx, &user, "deleted", &t, c.into_value()).await?;
    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Deserialize)]
pub struct LinkBody {
    url: String,
    title: Option<String>,
}

pub async fn add_link(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    body: web::Json<LinkBody>,
) -> ApiResult<HttpResponse> {
    Ok(HttpResponse::Created().json(service::add_link(&state.db, &user, *path, &body.url, body.title.as_deref()).await?))
}

pub async fn delete_link(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    let row: Option<(Uuid, String, Option<String>)> =
        sqlx::query_as("SELECT work_item_id, url, title FROM work_item_links WHERE id = $1")
            .bind(*path)
            .fetch_optional(&state.db)
            .await?;
    let (item, url, title) = row.ok_or(ApiError::NotFound)?;
    access::work_item(&state.db, item, &user).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM work_item_links WHERE id = $1").bind(*path).execute(&mut *tx).await?;
    let target = Target::on_item(&activity::item_target(&mut tx, item).await?, "link", *path);
    let mut c = Changes::new();
    c.value("url", &url).value("title", &title);
    activity::record(&mut tx, &user, "deleted", &target, c.into_value()).await?;
    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Deserialize)]
pub struct ReactionBody {
    emoji: String,
}

/// Toggles the user's reaction and returns the item's reactions.
pub async fn toggle_reaction(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    body: web::Json<ReactionBody>,
) -> ApiResult<HttpResponse> {
    access::work_item(&state.db, *path, &user).await?;
    let emoji = body.emoji.trim();
    if emoji.is_empty() || emoji.chars().count() > 16 || emoji.chars().any(char::is_alphanumeric) {
        return Err(bad("reactions must be an emoji"));
    }
    let mut tx = state.db.begin().await?;
    let removed = sqlx::query("DELETE FROM work_item_reactions WHERE work_item_id = $1 AND user_id = $2 AND emoji = $3")
        .bind(*path)
        .bind(user.id)
        .bind(emoji)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if removed == 0 {
        sqlx::query("INSERT INTO work_item_reactions (work_item_id, user_id, emoji) VALUES ($1, $2, $3)")
            .bind(*path)
            .bind(user.id)
            .bind(emoji)
            .execute(&mut *tx)
            .await?;
    }
    let item = activity::item_target(&mut tx, *path).await?;
    let target = Target::on_item(&item, "reaction", *path);
    let action = if removed == 0 { "created" } else { "deleted" };
    activity::record(&mut tx, &user, action, &target, Changes::new().value("emoji", emoji).take()).await?;
    tx.commit().await?;
    Ok(HttpResponse::Ok().json(service::reactions(&state.db, *path, user.id).await?))
}

pub async fn add_attachment(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    payload: Multipart,
) -> ApiResult<HttpResponse> {
    access::work_item(&state.db, *path, &user).await?;
    let file = files::store_upload(&state, &user, payload).await?;
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO work_item_attachments (work_item_id, file_id, created_by) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(*path)
    .bind(file.id)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("UPDATE work_items SET updated_at = now() WHERE id = $1").bind(*path).execute(&mut *tx).await?;
    let target = Target::on_item(&activity::item_target(&mut tx, *path).await?, "attachment", id);
    activity::record(&mut tx, &user, "created", &target, Changes::new().value("file_name", &file.file_name).take()).await?;
    tx.commit().await?;
    Ok(HttpResponse::Created().json(service::detail(&state.db, *path, &user).await?))
}

pub async fn delete_attachment(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    let row: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT a.work_item_id, a.file_id, f.file_name FROM work_item_attachments a
         JOIN files f ON f.id = a.file_id WHERE a.id = $1",
    )
    .bind(*path)
    .fetch_optional(&state.db)
    .await?;
    let (item, file, file_name) = row.ok_or(ApiError::NotFound)?;
    access::work_item(&state.db, item, &user).await?;
    {
        let mut tx = state.db.begin().await?;
        let target = Target::on_item(&activity::item_target(&mut tx, item).await?, "attachment", *path);
        activity::record(&mut tx, &user, "deleted", &target, Changes::new().value("file_name", &file_name).take()).await?;
        tx.commit().await?;
    }
    // Removing the file deletes the attachment row with it (and the object).
    files::delete_file(&state, file).await?;
    Ok(HttpResponse::NoContent().finish())
}

// ---- History -------------------------------------------------------------------

pub async fn item_activity(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    page: web::Query<Page>,
) -> ApiResult<HttpResponse> {
    access::work_item(&state.db, *path, &user).await?;
    Ok(HttpResponse::Ok().json(activity::list(&state.db, Scope::WorkItem(*path), &page).await?))
}

pub async fn description_history(state: web::Data<AppState>, user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    access::work_item(&state.db, *path, &user).await?;
    Ok(HttpResponse::Ok().json(activity::description_history(&state.db, *path).await?))
}

pub async fn project_activity(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    page: web::Query<Page>,
) -> ApiResult<HttpResponse> {
    access::project(&state.db, *path, &user).await?;
    Ok(HttpResponse::Ok().json(activity::list(&state.db, Scope::Project(*path), &page).await?))
}

pub async fn workspace_activity(
    state: web::Data<AppState>,
    user: AuthUser,
    path: web::Path<Uuid>,
    page: web::Query<Page>,
) -> ApiResult<HttpResponse> {
    access::workspace(&state.db, *path, &user).await?;
    Ok(HttpResponse::Ok().json(activity::list(&state.db, Scope::Workspace(*path), &page).await?))
}

pub async fn revert(state: web::Data<AppState>, user: AuthUser, path: web::Path<i64>) -> ApiResult<HttpResponse> {
    Ok(HttpResponse::Ok().json(activity::revert(&state.db, &user, *path).await?))
}
