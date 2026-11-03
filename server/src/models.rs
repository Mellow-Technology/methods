//! Rows returned by the API, and validation of enumerated values.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use crate::error::{ApiResult, bad};

pub const STATES: [&str; 5] = ["backlog", "todo", "in_progress", "done", "cancelled"];
pub const PRIORITIES: [&str; 4] = ["urgent", "high", "medium", "low"];
pub const RELATION_KINDS: [&str; 4] = ["relates_to", "duplicate_of", "blocked_by", "blocks"];

pub fn check_state(s: &str) -> ApiResult<()> {
    STATES.contains(&s).then_some(()).ok_or_else(|| bad(format!("state must be one of {}", STATES.join(", "))))
}

pub fn check_priority(s: &str) -> ApiResult<()> {
    PRIORITIES
        .contains(&s)
        .then_some(())
        .ok_or_else(|| bad(format!("priority must be one of {}", PRIORITIES.join(", "))))
}

pub fn check_relation(s: &str) -> ApiResult<()> {
    RELATION_KINDS
        .contains(&s)
        .then_some(())
        .ok_or_else(|| bad(format!("relation kind must be one of {}", RELATION_KINDS.join(", "))))
}

pub fn non_empty(field: &str, value: &str) -> ApiResult<String> {
    let v = value.trim();
    if v.is_empty() {
        return Err(bad(format!("{field} is required")));
    }
    if v.chars().count() > 500 {
        return Err(bad(format!("{field} is too long")));
    }
    Ok(v.to_string())
}

/// For PATCH bodies: distinguishes a missing field (`None`) from an explicit
/// `null` (`Some(None)`).
pub fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}


#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct UserOut {
    pub id: Uuid,
    pub kind: String,
    pub display_name: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    pub avatar_file_id: Option<Uuid>,
    pub avatar_url: Option<String>,
    pub cover_file_id: Option<Uuid>,
    pub cover_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub const USER_COLUMNS: &str = "u.id, u.kind, u.display_name, u.first_name, u.last_name, u.email,
    u.avatar_file_id, '/api/files/' || u.avatar_file_id::text AS avatar_url,
    u.cover_file_id, '/api/files/' || u.cover_file_id::text AS cover_url, u.created_at";

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Member {
    pub id: Uuid,
    pub kind: String,
    pub display_name: String,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub role: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Workspace {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub role: String,
    pub project_count: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Project {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub identifier: String,
    pub description: String,
    pub icon_emoji: Option<String>,
    pub icon_file_id: Option<Uuid>,
    pub icon_url: Option<String>,
    pub cover_file_id: Option<Uuid>,
    pub cover_url: Option<String>,
    /// `{ "todo": 3, "done": 5, ... }`
    pub state_counts: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub const PROJECT_COLUMNS: &str = "p.id, p.workspace_id, p.name, p.identifier, p.description, p.icon_emoji,
    p.icon_file_id, '/api/files/' || p.icon_file_id::text AS icon_url,
    p.cover_file_id, '/api/files/' || p.cover_file_id::text AS cover_url,
    COALESCE((SELECT json_object_agg(s.state, s.n) FROM
        (SELECT state, count(*) AS n FROM work_items WHERE project_id = p.id GROUP BY state) s), '{}'::json) AS state_counts,
    p.created_at, p.updated_at";

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Label {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Epic {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub description: String,
    pub color: String,
    pub item_count: i64,
    pub done_count: i64,
    pub created_at: DateTime<Utc>,
}

pub const EPIC_COLUMNS: &str = "e.id, e.project_id, e.name, e.description, e.color,
    (SELECT count(*) FROM work_items w WHERE w.epic_id = e.id) AS item_count,
    (SELECT count(*) FROM work_items w WHERE w.epic_id = e.id AND w.state IN ('done', 'cancelled')) AS done_count,
    e.created_at";

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct WorkItemSummary {
    pub id: Uuid,
    pub project_id: Uuid,
    pub seq: i32,
    pub key: String,
    pub title: String,
    pub state: String,
    pub priority: String,
    pub assignee_id: Option<Uuid>,
    pub assignee_name: Option<String>,
    pub assignee_kind: Option<String>,
    pub assignee_avatar_url: Option<String>,
    pub created_by: Option<Uuid>,
    pub start_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub parent_id: Option<Uuid>,
    pub epic_id: Option<Uuid>,
    pub sort_order: f64,
    pub label_ids: Vec<Uuid>,
    pub comment_count: i64,
    pub attachment_count: i64,
    pub child_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub const WORK_ITEM_SELECT: &str = "SELECT w.id, w.project_id, w.seq, p.identifier || '-' || w.seq AS key,
    w.title, w.state, w.priority,
    w.assignee_id, a.display_name AS assignee_name, a.kind AS assignee_kind,
    '/api/files/' || a.avatar_file_id::text AS assignee_avatar_url,
    w.created_by, w.start_date, w.due_date, w.parent_id, w.epic_id, w.sort_order,
    COALESCE((SELECT array_agg(l.label_id) FROM work_item_labels l WHERE l.work_item_id = w.id), '{}') AS label_ids,
    (SELECT count(*) FROM comments c WHERE c.work_item_id = w.id) AS comment_count,
    (SELECT count(*) FROM work_item_attachments t WHERE t.work_item_id = w.id) AS attachment_count,
    (SELECT count(*) FROM work_items ch WHERE ch.parent_id = w.id) AS child_count,
    w.created_at, w.updated_at
    FROM work_items w
    JOIN projects p ON p.id = w.project_id
    LEFT JOIN users a ON a.id = w.assignee_id";

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ItemRef {
    pub id: Uuid,
    pub key: String,
    pub title: String,
    pub state: String,
    pub priority: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Relation {
    pub id: Uuid,
    /// From the point of view of the item being viewed: relates_to,
    /// duplicate_of, duplicated_by, blocks or blocked_by.
    pub kind: String,
    pub item_id: Uuid,
    pub item_key: String,
    pub item_title: String,
    pub item_state: String,
    pub item_priority: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Link {
    pub id: Uuid,
    pub url: String,
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct FileOut {
    pub id: Uuid,
    pub url: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Attachment {
    pub id: Uuid,
    pub file_id: Uuid,
    pub url: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub created_by_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ReactionGroup {
    pub emoji: String,
    pub count: i64,
    pub reacted: bool,
    pub users: Vec<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Comment {
    pub id: Uuid,
    pub work_item_id: Uuid,
    pub author_id: Option<Uuid>,
    pub author_name: Option<String>,
    pub author_kind: Option<String>,
    pub author_avatar_url: Option<String>,
    /// Rich text document (see richtext.rs).
    pub body: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub const COMMENT_SELECT: &str = "SELECT c.id, c.work_item_id, c.author_id, u.display_name AS author_name,
    u.kind AS author_kind, '/api/files/' || u.avatar_file_id::text AS author_avatar_url,
    c.body, c.created_at, c.updated_at
    FROM comments c LEFT JOIN users u ON u.id = c.author_id";

#[derive(Debug, Serialize)]
pub struct WorkItemDetail {
    #[serde(flatten)]
    pub item: WorkItemSummary,
    /// Rich text document (see richtext.rs), or null for none.
    pub description: Option<serde_json::Value>,
    pub created_by_name: Option<String>,
    pub labels: Vec<Label>,
    pub epic: Option<Epic>,
    pub parent: Option<ItemRef>,
    pub children: Vec<ItemRef>,
    pub relations: Vec<Relation>,
    pub links: Vec<Link>,
    pub attachments: Vec<Attachment>,
    pub reactions: Vec<ReactionGroup>,
}
