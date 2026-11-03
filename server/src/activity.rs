//! The audit trail: who did what to which object, and when.
//!
//! Every change in a workspace writes one `activity` row in the same
//! transaction as the change, with the old and new values of what changed.
//! That one table serves three purposes:
//!
//! - history: feeds per work item, project and workspace;
//! - rollback: an entry's old values can be reapplied ([`revert`]), and
//!   deleted items and comments restored;
//! - real time: each row is announced with `pg_notify` when the transaction
//!   commits, and `live.rs` streams it to browsers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::{
    access,
    auth::AuthUser,
    error::{ApiError, ApiResult, bad},
    richtext,
    service::{self, UpdateWorkItem},
};

/// The Postgres notification channel `live.rs` listens on.
pub const CHANNEL: &str = "methods_events";

/// What an entry is about.
#[derive(Debug, Clone)]
pub struct Target {
    pub workspace_id: Uuid,
    pub project_id: Option<Uuid>,
    pub work_item_id: Option<Uuid>,
    pub related_item_id: Option<Uuid>,
    pub entity_type: &'static str,
    pub entity_id: Uuid,
    pub label: String,
}

impl Target {
    /// Something that belongs to a work item (a comment, link, …), labelled
    /// with the item.
    pub fn on_item(item: &Target, entity_type: &'static str, entity_id: Uuid) -> Target {
        Target { entity_type, entity_id, ..item.clone() }
    }
}

/// A work item, deleted or not. Labelled "MTH-12 Fix login".
pub async fn item_target(conn: &mut PgConnection, id: Uuid) -> ApiResult<Target> {
    let (workspace_id, project_id, label): (Uuid, Uuid, String) = sqlx::query_as(
        "SELECT p.workspace_id, w.project_id, p.identifier || '-' || w.seq || ' ' || w.title
         FROM work_items_all w JOIN projects p ON p.id = w.project_id WHERE w.id = $1",
    )
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or(ApiError::NotFound)?;
    Ok(Target {
        workspace_id,
        project_id: Some(project_id),
        work_item_id: Some(id),
        related_item_id: None,
        entity_type: "work_item",
        entity_id: id,
        label,
    })
}

/// Something in a project (the project itself, a label, an epic).
pub async fn project_target(
    conn: &mut PgConnection,
    project_id: Uuid,
    entity_type: &'static str,
    entity_id: Uuid,
    label: String,
) -> ApiResult<Target> {
    let workspace_id: Uuid = sqlx::query_scalar("SELECT workspace_id FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(conn)
        .await?;
    Ok(Target {
        workspace_id,
        project_id: Some(project_id),
        work_item_id: None,
        related_item_id: None,
        entity_type,
        entity_id,
        label,
    })
}

/// Something that belongs to the workspace itself (members, bots, tokens).
pub fn workspace_target(workspace_id: Uuid, entity_type: &'static str, entity_id: Uuid, label: String) -> Target {
    Target { workspace_id, project_id: None, work_item_id: None, related_item_id: None, entity_type, entity_id, label }
}

/// Builds the `changes` object, skipping fields that didn't change.
#[derive(Default)]
pub struct Changes(Map<String, Value>);

impl Changes {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, field: &str, from: impl Serialize, to: impl Serialize) -> &mut Self {
        let (from, to) = (json!(from), json!(to));
        if from != to {
            self.0.insert(field.into(), json!({ "from": from, "to": to }));
        }
        self
    }

    /// A value for a created or deleted object (no before/after).
    pub fn value(&mut self, field: &str, value: impl Serialize) -> &mut Self {
        let v = json!(value);
        if !v.is_null() {
            self.0.insert(field.into(), json!({ "to": v }));
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn into_value(self) -> Value {
        Value::Object(self.0)
    }

    /// For one-line chains: `Changes::new().value("x", 1).take()`.
    pub fn take(&mut self) -> Value {
        Value::Object(std::mem::take(&mut self.0))
    }
}

/// A real-time event: an activity entry (`id` set), or a change that isn't
/// worth a history entry, such as reordering cards (`id` null).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: Option<i64>,
    pub workspace_id: Uuid,
    pub project_id: Option<Uuid>,
    pub work_item_id: Option<Uuid>,
    pub related_item_id: Option<Uuid>,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub action: String,
    pub actor_id: Option<Uuid>,
}

impl Event {
    pub fn for_target(id: Option<i64>, t: &Target, action: &str, actor: Option<Uuid>) -> Event {
        Event {
            id,
            workspace_id: t.workspace_id,
            project_id: t.project_id,
            work_item_id: t.work_item_id,
            related_item_id: t.related_item_id,
            entity_type: t.entity_type.into(),
            entity_id: t.entity_id,
            action: action.into(),
            actor_id: actor,
        }
    }
}

/// Announces an event. Delivered when the transaction commits, and dropped
/// if it rolls back.
pub async fn notify(conn: &mut PgConnection, event: &Event) -> ApiResult<()> {
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(CHANNEL)
        .bind(serde_json::to_string(event).unwrap_or_default())
        .execute(conn)
        .await?;
    Ok(())
}

/// Records an entry and announces it. Call inside the change's transaction.
pub async fn record(
    conn: &mut PgConnection,
    actor: &AuthUser,
    action: &str,
    target: &Target,
    changes: Value,
) -> ApiResult<i64> {
    record_revert(conn, actor, action, target, changes, None).await
}

async fn record_revert(
    conn: &mut PgConnection,
    actor: &AuthUser,
    action: &str,
    target: &Target,
    changes: Value,
    reverts: Option<i64>,
) -> ApiResult<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO activity (workspace_id, project_id, work_item_id, related_item_id, actor_id, actor_name,
                               actor_kind, action, entity_type, entity_id, entity_label, changes, reverts)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) RETURNING id",
    )
    .bind(target.workspace_id)
    .bind(target.project_id)
    .bind(target.work_item_id)
    .bind(target.related_item_id)
    .bind(actor.id)
    .bind(&actor.display_name)
    .bind(&actor.kind)
    .bind(action)
    .bind(target.entity_type)
    .bind(target.entity_id)
    .bind(&target.label)
    .bind(changes)
    .bind(reverts)
    .fetch_one(&mut *conn)
    .await?;
    if let Some(original) = reverts {
        sqlx::query("UPDATE activity SET reverted_by = $1 WHERE id = $2").bind(id).bind(original).execute(&mut *conn).await?;
    }
    notify(conn, &Event::for_target(Some(id), target, action, Some(actor.id))).await?;
    Ok(id)
}

// ---- Work item changes --------------------------------------------------------

/// The fields of a work item that history tracks, as JSON, in the same shape
/// `UpdateWorkItem` accepts, so old values can be fed straight back in.
pub async fn item_fields(conn: &mut PgConnection, id: Uuid) -> ApiResult<Map<String, Value>> {
    let v: Value = sqlx::query_scalar(
        "SELECT jsonb_build_object(
            'title', title, 'description', description, 'state', state, 'priority', priority,
            'assignee_id', assignee_id, 'start_date', start_date, 'due_date', due_date,
            'parent_id', parent_id, 'epic_id', epic_id,
            'label_ids', COALESCE((SELECT jsonb_agg(label_id ORDER BY label_id)
                                   FROM work_item_labels WHERE work_item_id = w.id), '[]'::jsonb))
         FROM work_items_all w WHERE id = $1",
    )
    .bind(id)
    .fetch_one(conn)
    .await?;
    Ok(v.as_object().cloned().unwrap_or_default())
}

/// The difference between two snapshots from [`item_fields`], with
/// readable names for ids (assignee, epic, parent, labels).
pub async fn item_diff(conn: &mut PgConnection, before: &Map<String, Value>, after: &Map<String, Value>) -> ApiResult<Value> {
    let mut out = Map::new();
    for (field, to) in after {
        let from = before.get(field).unwrap_or(&Value::Null);
        if from == to {
            continue;
        }
        let mut entry = json!({ "from": from, "to": to });
        if let (Some(from_label), Some(to_label)) = (name_of(conn, field, from).await?, name_of(conn, field, to).await?) {
            entry["from_label"] = from_label;
            entry["to_label"] = to_label;
        }
        out.insert(field.clone(), entry);
    }
    Ok(Value::Object(out))
}

/// A readable name for an id-valued field, or `None` for other fields.
async fn name_of(conn: &mut PgConnection, field: &str, v: &Value) -> ApiResult<Option<Value>> {
    let sql = match field {
        "assignee_id" => "SELECT display_name FROM users WHERE id = $1",
        "epic_id" => "SELECT name FROM epics WHERE id = $1",
        "parent_id" => {
            "SELECT p.identifier || '-' || w.seq || ' ' || w.title FROM work_items_all w
             JOIN projects p ON p.id = w.project_id WHERE w.id = $1"
        }
        "label_ids" => {
            let ids: Vec<Uuid> = serde_json::from_value(v.clone()).unwrap_or_default();
            let names: Vec<String> =
                sqlx::query_scalar("SELECT name FROM labels WHERE id = ANY($1) ORDER BY lower(name)")
                    .bind(&ids)
                    .fetch_all(conn)
                    .await?;
            return Ok(Some(json!(names)));
        }
        _ => return Ok(None),
    };
    let Some(id) = v.as_str().and_then(|s| Uuid::parse_str(s).ok()) else {
        return Ok(Some(Value::Null));
    };
    let name: Option<String> = sqlx::query_scalar(sql).bind(id).fetch_optional(conn).await?;
    Ok(Some(json!(name)))
}

/// Records a work item update. Successive description edits by the same
/// person within a few minutes are folded into one entry, so typing in
/// bursts doesn't flood the history.
pub async fn record_item_update(
    conn: &mut PgConnection,
    actor: &AuthUser,
    target: &Target,
    changes: Value,
    reverts: Option<i64>,
) -> ApiResult<()> {
    let only_description = changes.as_object().is_some_and(|c| c.len() == 1 && c.contains_key("description"));
    if only_description && reverts.is_none() {
        let folded: Option<i64> = sqlx::query_scalar(
            "UPDATE activity SET changes = jsonb_set(changes, '{description,to}', $3), created_at = now()
             WHERE id = (SELECT id FROM activity WHERE work_item_id = $1 AND entity_type = 'work_item'
                         ORDER BY id DESC LIMIT 1)
               AND actor_id = $2 AND action = 'updated' AND reverted_by IS NULL
               AND changes ? 'description' AND (SELECT count(*) FROM jsonb_object_keys(changes)) = 1
               AND created_at > now() - interval '10 minutes'
             RETURNING id",
        )
        .bind(target.entity_id)
        .bind(actor.id)
        .bind(&changes["description"]["to"])
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(id) = folded {
            return notify(conn, &Event::for_target(Some(id), target, "updated", Some(actor.id))).await;
        }
    }
    let action = if reverts.is_some() { "reverted" } else { "updated" };
    record_revert(conn, actor, action, target, changes, reverts).await?;
    Ok(())
}

// ---- Feeds -------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Entry {
    pub id: i64,
    pub workspace_id: Uuid,
    pub project_id: Option<Uuid>,
    pub work_item_id: Option<Uuid>,
    pub related_item_id: Option<Uuid>,
    pub actor_id: Option<Uuid>,
    pub actor_name: String,
    pub actor_kind: String,
    pub actor_avatar_url: Option<String>,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub entity_label: String,
    pub changes: Value,
    pub reverts: Option<i64>,
    pub reverted_by: Option<i64>,
    pub created_at: DateTime<Utc>,
}

const ENTRY_SELECT: &str = "SELECT a.id, a.workspace_id, a.project_id, a.work_item_id, a.related_item_id,
    a.actor_id, COALESCE(u.display_name, a.actor_name) AS actor_name, a.actor_kind,
    '/api/files/' || u.avatar_file_id::text AS actor_avatar_url, a.action, a.entity_type, a.entity_id,
    a.entity_label, a.changes, a.reverts, a.reverted_by, a.created_at
    FROM activity a LEFT JOIN users u ON u.id = a.actor_id";

pub enum Scope {
    Workspace(Uuid),
    Project(Uuid),
    WorkItem(Uuid),
}

#[derive(Deserialize)]
pub struct Page {
    /// Entries older than this id.
    pub before: Option<i64>,
    pub limit: Option<i64>,
}

pub async fn list(db: &PgPool, scope: Scope, page: &Page) -> ApiResult<Vec<Value>> {
    let (cond, id) = match scope {
        Scope::Workspace(id) => ("a.workspace_id = $1", id),
        Scope::Project(id) => ("a.project_id = $1", id),
        Scope::WorkItem(id) => ("(a.work_item_id = $1 OR a.related_item_id = $1)", id),
    };
    let rows = sqlx::query_as::<_, Entry>(&format!(
        "{ENTRY_SELECT} WHERE {cond} AND a.id < $2 ORDER BY a.id DESC LIMIT $3"
    ))
    .bind(id)
    .bind(page.before.unwrap_or(i64::MAX))
    .bind(page.limit.unwrap_or(50).clamp(1, 200))
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(present).collect())
}

/// Whether an entry can be undone from the feed.
fn revertible(e: &Entry) -> bool {
    e.reverted_by.is_none()
        && matches!(
            (e.entity_type.as_str(), e.action.as_str()),
            ("work_item", "updated" | "reverted" | "deleted") | ("comment", "updated" | "reverted" | "deleted")
        )
}

/// An entry as clients see it: rich text documents become short Markdown
/// excerpts (full versions come from the description history), and it says
/// whether it can be reverted.
pub fn present(e: Entry) -> Value {
    let can_revert = revertible(&e);
    let mut v = serde_json::to_value(&e).unwrap_or_default();
    if let Some(changes) = v.get_mut("changes").and_then(Value::as_object_mut) {
        for change in changes.values_mut() {
            for side in ["from", "to"] {
                if let Some(doc) = change.get(side).filter(|d| d.get("type").and_then(Value::as_str) == Some("doc")) {
                    let md = richtext::to_markdown(doc);
                    let excerpt: String = md.chars().take(240).collect();
                    change[side] = json!(if excerpt.len() < md.len() { format!("{excerpt}…") } else { excerpt });
                    change["rich"] = json!(true);
                }
            }
        }
    }
    v["revertible"] = json!(can_revert);
    v
}

/// Every saved version of an item's description, newest first.
pub async fn description_history(db: &PgPool, item_id: Uuid) -> ApiResult<Vec<Value>> {
    let rows = sqlx::query_as::<_, Entry>(&format!(
        "{ENTRY_SELECT} WHERE a.work_item_id = $1 AND a.entity_type = 'work_item'
         AND a.changes ? 'description' ORDER BY a.id DESC LIMIT 100"
    ))
    .bind(item_id)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|e| {
            json!({
                "activity_id": e.id,
                "actor_name": e.actor_name,
                "actor_kind": e.actor_kind,
                "actor_avatar_url": e.actor_avatar_url,
                "action": e.action,
                "created_at": e.created_at,
                "description": e.changes["description"]["to"],
            })
        })
        .collect())
}

// ---- Revert ----------------------------------------------------------------------

/// Undoes an entry: reapplies the old values of an update (if nothing has
/// changed those fields since), or restores a deleted item or comment.
pub async fn revert(db: &PgPool, user: &AuthUser, id: i64) -> ApiResult<Value> {
    let entry = sqlx::query_as::<_, Entry>(&format!("{ENTRY_SELECT} WHERE a.id = $1"))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let access = access::workspace(db, entry.workspace_id, user).await?;
    if entry.reverted_by.is_some() {
        return Err(ApiError::Conflict("that change has already been undone".into()));
    }
    if !revertible(&entry) {
        return Err(bad("that change can't be undone"));
    }
    let changes = entry.changes.as_object().cloned().unwrap_or_default();

    match (entry.entity_type.as_str(), entry.action.as_str()) {
        ("work_item", "deleted") => {
            let mut tx = db.begin().await?;
            sqlx::query("UPDATE work_items_all SET deleted_at = NULL, deleted_by = NULL WHERE id = $1")
                .bind(entry.entity_id)
                .execute(&mut *tx)
                .await?;
            let target = item_target(&mut tx, entry.entity_id).await?;
            record_revert(&mut tx, user, "restored", &target, json!({}), Some(entry.id)).await?;
            tx.commit().await?;
            Ok(json!({ "work_item_id": entry.entity_id }))
        }
        ("comment", "deleted") => {
            if entry.actor_id != Some(user.id) && !access.is_admin() {
                return Err(ApiError::Forbidden);
            }
            let mut tx = db.begin().await?;
            let item: Uuid = sqlx::query_scalar(
                "UPDATE comments_all SET deleted_at = NULL, deleted_by = NULL WHERE id = $1 RETURNING work_item_id",
            )
            .bind(entry.entity_id)
            .fetch_one(&mut *tx)
            .await?;
            // The item itself may have been deleted since.
            access::work_item(db, item, user).await?;
            let target = Target::on_item(&item_target(&mut tx, item).await?, "comment", entry.entity_id);
            record_revert(&mut tx, user, "restored", &target, json!({}), Some(entry.id)).await?;
            tx.commit().await?;
            Ok(json!({ "work_item_id": item }))
        }
        ("work_item", _) => {
            let mut conn = db.acquire().await?;
            let current = item_fields(&mut conn, entry.entity_id).await?;
            drop(conn);
            // Only undo if each field still holds what this change set it to;
            // otherwise someone changed it since and undoing would clobber that.
            let moved: Vec<&str> = changes
                .iter()
                .filter(|(f, c)| current.get(*f).unwrap_or(&Value::Null) != c.get("to").unwrap_or(&Value::Null))
                .map(|(f, _)| f.as_str())
                .collect();
            if !moved.is_empty() {
                return Err(ApiError::Conflict(format!(
                    "can't undo: {} changed again since",
                    moved.iter().map(|f| f.trim_end_matches("_id").replace('_', " ")).collect::<Vec<_>>().join(", ")
                )));
            }
            let old: Map<String, Value> =
                changes.iter().map(|(f, c)| (f.clone(), c.get("from").cloned().unwrap_or(Value::Null))).collect();
            let input: UpdateWorkItem =
                serde_json::from_value(Value::Object(old)).map_err(|e| bad(format!("can't undo: {e}")))?;
            let detail = service::update_reverting(db, user, entry.entity_id, input, Some(entry.id)).await?;
            Ok(json!({ "work_item_id": entry.entity_id, "work_item": detail }))
        }
        ("comment", _) => {
            if entry.actor_id != Some(user.id) {
                return Err(ApiError::Forbidden);
            }
            let (Some(from), Some(to)) = (changes.get("body").and_then(|c| c.get("from")), changes.get("body").and_then(|c| c.get("to")))
            else {
                return Err(bad("that change can't be undone"));
            };
            let mut tx = db.begin().await?;
            let current: Option<(Value, Uuid)> =
                sqlx::query_as("SELECT body, work_item_id FROM comments WHERE id = $1 FOR UPDATE")
                    .bind(entry.entity_id)
                    .fetch_optional(&mut *tx)
                    .await?;
            let (body, item) = current.ok_or(ApiError::NotFound)?;
            if &body != to {
                return Err(ApiError::Conflict("can't undo: the comment changed again since".into()));
            }
            sqlx::query("UPDATE comments SET body = $1, updated_at = now() WHERE id = $2")
                .bind(from)
                .bind(entry.entity_id)
                .execute(&mut *tx)
                .await?;
            let target = Target::on_item(&item_target(&mut tx, item).await?, "comment", entry.entity_id);
            let mut c = Changes::new();
            c.set("body", to, from);
            record_revert(&mut tx, user, "reverted", &target, c.into_value(), Some(entry.id)).await?;
            tx.commit().await?;
            Ok(json!({ "work_item_id": item }))
        }
        _ => Err(bad("that change can't be undone")),
    }
}
