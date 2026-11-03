//! Authorization. Everything lives in a workspace; a user may see and change
//! what's in the workspaces they're a member of. Owners and admins may also
//! manage members, bots and projects. Records outside the user's workspaces
//! are reported as not found so their existence isn't revealed.

use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    auth::AuthUser,
    error::{ApiError, ApiResult},
};

#[derive(Debug, Clone)]
pub struct Access {
    pub workspace_id: Uuid,
    pub role: String,
}

impl Access {
    pub fn is_admin(&self) -> bool {
        self.role == "owner" || self.role == "admin"
    }

    pub fn require_admin(&self) -> ApiResult<()> {
        if self.is_admin() { Ok(()) } else { Err(ApiError::Forbidden) }
    }
}

async fn fetch(db: &PgPool, sql: &str, id: Uuid, user: &AuthUser) -> ApiResult<Access> {
    let row: Option<(Uuid, String)> = sqlx::query_as(sql).bind(id).bind(user.id).fetch_optional(db).await?;
    row.map(|(workspace_id, role)| Access { workspace_id, role }).ok_or(ApiError::NotFound)
}

pub async fn workspace(db: &PgPool, workspace_id: Uuid, user: &AuthUser) -> ApiResult<Access> {
    fetch(
        db,
        "SELECT workspace_id, role FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
        workspace_id,
        user,
    )
    .await
}

pub async fn project(db: &PgPool, project_id: Uuid, user: &AuthUser) -> ApiResult<Access> {
    fetch(
        db,
        "SELECT m.workspace_id, m.role FROM projects p
         JOIN workspace_members m ON m.workspace_id = p.workspace_id AND m.user_id = $2
         WHERE p.id = $1",
        project_id,
        user,
    )
    .await
}

/// Returns the item's project id along with the access.
pub async fn work_item(db: &PgPool, item_id: Uuid, user: &AuthUser) -> ApiResult<(Uuid, Access)> {
    let row: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT w.project_id, m.workspace_id, m.role FROM work_items w
         JOIN projects p ON p.id = w.project_id
         JOIN workspace_members m ON m.workspace_id = p.workspace_id AND m.user_id = $2
         WHERE w.id = $1",
    )
    .bind(item_id)
    .bind(user.id)
    .fetch_optional(db)
    .await?;
    row.map(|(project_id, workspace_id, role)| (project_id, Access { workspace_id, role }))
        .ok_or(ApiError::NotFound)
}

/// Resolves the project a child record (label, epic) belongs to.
pub async fn via(db: &PgPool, table: &str, id: Uuid, user: &AuthUser) -> ApiResult<(Uuid, Access)> {
    // `table` is always a literal from our own code.
    let sql = format!(
        "SELECT x.project_id, m.workspace_id, m.role FROM {table} x
         JOIN projects p ON p.id = x.project_id
         JOIN workspace_members m ON m.workspace_id = p.workspace_id AND m.user_id = $2
         WHERE x.id = $1"
    );
    let row: Option<(Uuid, Uuid, String)> =
        sqlx::query_as(&sql).bind(id).bind(user.id).fetch_optional(db).await?;
    row.map(|(project_id, workspace_id, role)| (project_id, Access { workspace_id, role }))
        .ok_or(ApiError::NotFound)
}

pub async fn is_member(db: &PgPool, workspace_id: Uuid, user_id: Uuid) -> ApiResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM workspace_members WHERE workspace_id = $1 AND user_id = $2)",
    )
    .bind(workspace_id)
    .bind(user_id)
    .fetch_one(db)
    .await?)
}
