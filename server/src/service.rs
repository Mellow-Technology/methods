//! Work item operations, shared by the REST API and the MCP endpoint.

use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::Value;
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::{
    access,
    activity::{self, Changes, Target},
    auth::AuthUser,
    error::{ApiError, ApiResult, bad},
    models::*,
    richtext,
};

#[derive(Debug, Default, Deserialize)]
pub struct WorkItemFilter {
    pub state: Option<String>,
    pub assignee_id: Option<Uuid>,
    pub epic_id: Option<Uuid>,
    pub label_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    /// Matches the title or the key (e.g. "MTH-12").
    pub q: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CreateWorkItem {
    pub title: String,
    /// A rich text document, or Markdown.
    #[serde(default)]
    pub description: Option<Value>,
    pub state: Option<String>,
    pub priority: Option<String>,
    pub assignee_id: Option<Uuid>,
    pub start_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub parent_id: Option<Uuid>,
    pub epic_id: Option<Uuid>,
    #[serde(default)]
    pub label_ids: Vec<Uuid>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateWorkItem {
    pub title: Option<String>,
    /// A rich text document, Markdown, or null to clear.
    #[serde(default, deserialize_with = "double_option")]
    pub description: Option<Option<Value>>,
    pub state: Option<String>,
    pub priority: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub assignee_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub start_date: Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "double_option")]
    pub due_date: Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "double_option")]
    pub parent_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "double_option")]
    pub epic_id: Option<Option<Uuid>>,
    pub sort_order: Option<f64>,
    pub label_ids: Option<Vec<Uuid>>,
}

pub async fn list(db: &PgPool, project_id: Uuid, filter: &WorkItemFilter) -> ApiResult<Vec<WorkItemSummary>> {
    let mut q = QueryBuilder::<Postgres>::new(WORK_ITEM_SELECT);
    q.push(" WHERE w.project_id = ").push_bind(project_id);
    if let Some(state) = &filter.state {
        check_state(state)?;
        q.push(" AND w.state = ").push_bind(state.clone());
    }
    if let Some(id) = filter.assignee_id {
        q.push(" AND w.assignee_id = ").push_bind(id);
    }
    if let Some(id) = filter.epic_id {
        q.push(" AND w.epic_id = ").push_bind(id);
    }
    if let Some(id) = filter.parent_id {
        q.push(" AND w.parent_id = ").push_bind(id);
    }
    if let Some(id) = filter.label_id {
        q.push(" AND EXISTS (SELECT 1 FROM work_item_labels wl WHERE wl.work_item_id = w.id AND wl.label_id = ")
            .push_bind(id)
            .push(")");
    }
    if let Some(text) = filter.q.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        let pattern = format!("%{}%", text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        q.push(" AND (w.title ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR (p.identifier || '-' || w.seq) ILIKE ")
            .push_bind(pattern)
            .push(")");
    }
    q.push(" ORDER BY w.state, w.sort_order, w.seq");
    q.push(" LIMIT ").push_bind(filter.limit.unwrap_or(2000).clamp(1, 5000));
    Ok(q.build_query_as::<WorkItemSummary>().fetch_all(db).await?)
}

pub async fn summary(db: &PgPool, id: Uuid) -> ApiResult<WorkItemSummary> {
    Ok(sqlx::query_as::<_, WorkItemSummary>(&format!("{WORK_ITEM_SELECT} WHERE w.id = $1"))
        .bind(id)
        .fetch_one(db)
        .await?)
}

pub async fn detail(db: &PgPool, id: Uuid, viewer: &AuthUser) -> ApiResult<WorkItemDetail> {
    let item = summary(db, id).await?;
    let (description, created_by_name): (Option<Value>, Option<String>) = sqlx::query_as(
        "SELECT w.description, u.display_name FROM work_items w LEFT JOIN users u ON u.id = w.created_by WHERE w.id = $1",
    )
    .bind(id)
    .fetch_one(db)
    .await?;

    let labels = sqlx::query_as::<_, Label>(
        "SELECT l.id, l.project_id, l.name, l.color FROM labels l
         JOIN work_item_labels wl ON wl.label_id = l.id WHERE wl.work_item_id = $1 ORDER BY lower(l.name)",
    )
    .bind(id)
    .fetch_all(db)
    .await?;

    let epic = match item.epic_id {
        Some(epic_id) => Some(
            sqlx::query_as::<_, Epic>(&format!("SELECT {EPIC_COLUMNS} FROM epics e WHERE e.id = $1"))
                .bind(epic_id)
                .fetch_one(db)
                .await?,
        ),
        None => None,
    };

    const REF: &str = "SELECT w.id, p.identifier || '-' || w.seq AS key, w.title, w.state, w.priority
        FROM work_items w JOIN projects p ON p.id = w.project_id";
    let parent = match item.parent_id {
        Some(pid) => sqlx::query_as::<_, ItemRef>(&format!("{REF} WHERE w.id = $1"))
            .bind(pid)
            .fetch_optional(db)
            .await?,
        None => None,
    };
    let children = sqlx::query_as::<_, ItemRef>(&format!("{REF} WHERE w.parent_id = $1 ORDER BY w.seq"))
        .bind(id)
        .fetch_all(db)
        .await?;

    let relations = sqlx::query_as::<_, Relation>(
        "SELECT r.id, r.kind, w.id AS item_id, p.identifier || '-' || w.seq AS item_key, w.title AS item_title,
                w.state AS item_state, w.priority AS item_priority
         FROM work_item_relations r JOIN work_items w ON w.id = r.target_id JOIN projects p ON p.id = w.project_id
         WHERE r.source_id = $1
         UNION ALL
         SELECT r.id,
                CASE r.kind WHEN 'blocks' THEN 'blocked_by' WHEN 'blocked_by' THEN 'blocks'
                            WHEN 'duplicate_of' THEN 'duplicated_by' ELSE r.kind END,
                w.id, p.identifier || '-' || w.seq, w.title, w.state, w.priority
         FROM work_item_relations r JOIN work_items w ON w.id = r.source_id JOIN projects p ON p.id = w.project_id
         WHERE r.target_id = $1",
    )
    .bind(id)
    .fetch_all(db)
    .await
    .map_err(ApiError::from)?;

    let links = sqlx::query_as::<_, Link>(
        "SELECT id, url, title, created_at FROM work_item_links WHERE work_item_id = $1 ORDER BY created_at",
    )
    .bind(id)
    .fetch_all(db)
    .await?;

    let attachments = sqlx::query_as::<_, Attachment>(
        "SELECT a.id, f.id AS file_id, '/api/files/' || f.id::text AS url, f.file_name, f.content_type,
                f.size_bytes, u.display_name AS created_by_name, a.created_at
         FROM work_item_attachments a JOIN files f ON f.id = a.file_id LEFT JOIN users u ON u.id = a.created_by
         WHERE a.work_item_id = $1 ORDER BY a.created_at",
    )
    .bind(id)
    .fetch_all(db)
    .await?;

    let reactions = reactions(db, id, viewer.id).await?;

    Ok(WorkItemDetail {
        item,
        description,
        created_by_name,
        labels,
        epic,
        parent,
        children,
        relations,
        links,
        attachments,
        reactions,
    })
}

pub async fn reactions(db: &PgPool, item_id: Uuid, viewer: Uuid) -> ApiResult<Vec<ReactionGroup>> {
    Ok(sqlx::query_as::<_, ReactionGroup>(
        "SELECT r.emoji, count(*) AS count, bool_or(r.user_id = $2) AS reacted,
                array_agg(u.display_name ORDER BY r.created_at) AS users
         FROM work_item_reactions r JOIN users u ON u.id = r.user_id
         WHERE r.work_item_id = $1 GROUP BY r.emoji ORDER BY min(r.created_at)",
    )
    .bind(item_id)
    .bind(viewer)
    .fetch_all(db)
    .await?)
}

async fn check_assignee(db: &PgPool, workspace_id: Uuid, assignee: Option<Uuid>) -> ApiResult<()> {
    if let Some(a) = assignee {
        if !access::is_member(db, workspace_id, a).await? {
            return Err(bad("the assignee isn't a member of this workspace"));
        }
    }
    Ok(())
}

async fn check_in_project(db: &PgPool, table: &str, id: Option<Uuid>, project_id: Uuid, what: &str) -> ApiResult<()> {
    if let Some(id) = id {
        let sql = format!("SELECT EXISTS (SELECT 1 FROM {table} WHERE id = $1 AND project_id = $2)");
        let ok: bool = sqlx::query_scalar(&sql).bind(id).bind(project_id).fetch_one(db).await?;
        if !ok {
            return Err(bad(format!("that {what} isn't in this project")));
        }
    }
    Ok(())
}

async fn check_labels(db: &PgPool, labels: &[Uuid], project_id: Uuid) -> ApiResult<()> {
    if labels.is_empty() {
        return Ok(());
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM labels WHERE id = ANY($1) AND project_id = $2")
        .bind(labels)
        .bind(project_id)
        .fetch_one(db)
        .await?;
    if n as usize != labels.len() {
        return Err(bad("one or more labels aren't in this project"));
    }
    Ok(())
}

async fn end_of_column(db: &PgPool, project_id: Uuid, state: &str) -> ApiResult<f64> {
    let max: Option<f64> =
        sqlx::query_scalar("SELECT max(sort_order) FROM work_items WHERE project_id = $1 AND state = $2")
            .bind(project_id)
            .bind(state)
            .fetch_one(db)
            .await?;
    Ok(max.unwrap_or(0.0) + 1024.0)
}

pub async fn create(db: &PgPool, user: &AuthUser, project_id: Uuid, input: CreateWorkItem) -> ApiResult<WorkItemDetail> {
    let access = access::project(db, project_id, user).await?;
    let title = non_empty("title", &input.title)?;
    let state = input.state.unwrap_or_else(|| "backlog".into());
    check_state(&state)?;
    let priority = input.priority.unwrap_or_else(|| "medium".into());
    check_priority(&priority)?;
    check_assignee(db, access.workspace_id, input.assignee_id).await?;
    check_in_project(db, "work_items", input.parent_id, project_id, "parent work item").await?;
    check_in_project(db, "epics", input.epic_id, project_id, "epic").await?;
    let mut labels = input.label_ids;
    labels.sort();
    labels.dedup();
    check_labels(db, &labels, project_id).await?;
    let sort_order = end_of_column(db, project_id, &state).await?;

    let mut tx = db.begin().await?;
    let seq: i32 =
        sqlx::query_scalar("UPDATE projects SET next_seq = next_seq + 1 WHERE id = $1 RETURNING next_seq - 1")
            .bind(project_id)
            .fetch_one(&mut *tx)
            .await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO work_items (project_id, seq, title, description, state, priority, assignee_id, created_by,
                                 start_date, due_date, parent_id, epic_id, sort_order)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) RETURNING id",
    )
    .bind(project_id)
    .bind(seq)
    .bind(title)
    .bind(rich(input.description.as_ref())?)
    .bind(&state)
    .bind(&priority)
    .bind(input.assignee_id)
    .bind(user.id)
    .bind(input.start_date)
    .bind(input.due_date)
    .bind(input.parent_id)
    .bind(input.epic_id)
    .bind(sort_order)
    .fetch_one(&mut *tx)
    .await?;
    if !labels.is_empty() {
        sqlx::query("INSERT INTO work_item_labels (work_item_id, label_id) SELECT $1, unnest($2::uuid[])")
            .bind(id)
            .bind(&labels)
            .execute(&mut *tx)
            .await?;
    }
    let target = activity::item_target(&mut tx, id).await?;
    let mut changes = Changes::new();
    for (field, value) in activity::item_fields(&mut tx, id).await? {
        if field != "label_ids" || value.as_array().is_some_and(|l| !l.is_empty()) {
            changes.value(&field, value);
        }
    }
    activity::record(&mut tx, user, "created", &target, changes.into_value()).await?;
    tx.commit().await?;
    detail(db, id, user).await
}

pub async fn update(db: &PgPool, user: &AuthUser, id: Uuid, input: UpdateWorkItem) -> ApiResult<WorkItemDetail> {
    update_reverting(db, user, id, input, None).await
}

/// An update, recorded as undoing activity entry `reverts` when given.
pub async fn update_reverting(
    db: &PgPool,
    user: &AuthUser,
    id: Uuid,
    input: UpdateWorkItem,
    reverts: Option<i64>,
) -> ApiResult<WorkItemDetail> {
    let (project_id, access) = access::work_item(db, id, user).await?;

    let mut q = QueryBuilder::<Postgres>::new("UPDATE work_items SET updated_at = now()");
    if let Some(title) = &input.title {
        q.push(", title = ").push_bind(non_empty("title", title)?);
    }
    if let Some(desc) = &input.description {
        q.push(", description = ").push_bind(rich(desc.as_ref())?);
    }
    if let Some(priority) = &input.priority {
        check_priority(priority)?;
        q.push(", priority = ").push_bind(priority.clone());
    }
    if let Some(state) = &input.state {
        check_state(state)?;
        q.push(", state = ").push_bind(state.clone());
        if input.sort_order.is_none() {
            let current: String = sqlx::query_scalar("SELECT state FROM work_items WHERE id = $1")
                .bind(id)
                .fetch_one(db)
                .await?;
            if &current != state {
                q.push(", sort_order = ").push_bind(end_of_column(db, project_id, state).await?);
            }
        }
    }
    if let Some(order) = input.sort_order {
        if !order.is_finite() {
            return Err(bad("sort_order must be a number"));
        }
        q.push(", sort_order = ").push_bind(order);
    }
    if let Some(assignee) = input.assignee_id {
        check_assignee(db, access.workspace_id, assignee).await?;
        q.push(", assignee_id = ").push_bind(assignee);
    }
    if let Some(start) = input.start_date {
        q.push(", start_date = ").push_bind(start);
    }
    if let Some(due) = input.due_date {
        q.push(", due_date = ").push_bind(due);
    }
    if let Some(parent) = input.parent_id {
        if let Some(pid) = parent {
            check_in_project(db, "work_items", Some(pid), project_id, "parent work item").await?;
            let cycle: bool = sqlx::query_scalar(
                "WITH RECURSIVE anc AS (
                     SELECT id, parent_id FROM work_items WHERE id = $1
                     UNION ALL
                     SELECT w.id, w.parent_id FROM work_items w JOIN anc ON w.id = anc.parent_id)
                 SELECT EXISTS (SELECT 1 FROM anc WHERE id = $2)",
            )
            .bind(pid)
            .bind(id)
            .fetch_one(db)
            .await?;
            if cycle {
                return Err(bad("an item can't be the parent of one of its own ancestors"));
            }
        }
        q.push(", parent_id = ").push_bind(parent);
    }
    if let Some(epic) = input.epic_id {
        check_in_project(db, "epics", epic, project_id, "epic").await?;
        q.push(", epic_id = ").push_bind(epic);
    }
    q.push(" WHERE id = ").push_bind(id);

    let mut tx = db.begin().await?;
    // Lock the row so the before/after snapshots bracket only this change.
    let old_order: f64 = sqlx::query_scalar("SELECT sort_order FROM work_items WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    let before = activity::item_fields(&mut tx, id).await?;
    q.build().execute(&mut *tx).await?;
    if let Some(mut labels) = input.label_ids {
        labels.sort();
        labels.dedup();
        check_labels(db, &labels, project_id).await?;
        sqlx::query("DELETE FROM work_item_labels WHERE work_item_id = $1").bind(id).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO work_item_labels (work_item_id, label_id) SELECT $1, unnest($2::uuid[])")
            .bind(id)
            .bind(&labels)
            .execute(&mut *tx)
            .await?;
    }
    let after = activity::item_fields(&mut tx, id).await?;
    let changes = activity::item_diff(&mut tx, &before, &after).await?;
    let target = activity::item_target(&mut tx, id).await?;
    if changes.as_object().is_some_and(|c| !c.is_empty()) {
        activity::record_item_update(&mut tx, user, &target, changes, reverts).await?;
    } else if input.sort_order.is_some_and(|o| o != old_order) {
        // Reordering within a column isn't history, but other boards
        // should still see it.
        activity::notify(&mut tx, &activity::Event::for_target(None, &target, "reordered", Some(user.id))).await?;
    }
    tx.commit().await?;
    detail(db, id, user).await
}

/// Moves an item to the trash; restoring it is undoing this entry.
pub async fn delete(db: &PgPool, user: &AuthUser, id: Uuid) -> ApiResult<()> {
    access::work_item(db, id, user).await?;
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE work_items SET deleted_at = now(), deleted_by = $2 WHERE id = $1")
        .bind(id)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    let target = activity::item_target(&mut tx, id).await?;
    activity::record(&mut tx, user, "deleted", &target, serde_json::json!({})).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn list_comments(db: &PgPool, user: &AuthUser, item_id: Uuid) -> ApiResult<Vec<Comment>> {
    access::work_item(db, item_id, user).await?;
    Ok(sqlx::query_as::<_, Comment>(&format!("{COMMENT_SELECT} WHERE c.work_item_id = $1 ORDER BY c.created_at"))
        .bind(item_id)
        .fetch_all(db)
        .await?)
}

/// Validates rich text input (a document or Markdown); `None` when empty.
pub fn rich(input: Option<&Value>) -> ApiResult<Option<Value>> {
    input.map_or(Ok(None), |v| richtext::from_input(v).map_err(bad))
}

/// A comment body: rich text that mustn't be empty.
pub fn comment_body(input: &Value) -> ApiResult<Value> {
    rich(Some(input))?.ok_or_else(|| bad("comment can't be empty"))
}

pub async fn add_comment(db: &PgPool, user: &AuthUser, item_id: Uuid, body: &Value) -> ApiResult<Comment> {
    access::work_item(db, item_id, user).await?;
    let body = comment_body(body)?;
    let mut tx = db.begin().await?;
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO comments (work_item_id, author_id, body) VALUES ($1, $2, $3) RETURNING id")
            .bind(item_id)
            .bind(user.id)
            .bind(&body)
            .fetch_one(&mut *tx)
            .await?;
    sqlx::query("UPDATE work_items SET updated_at = now() WHERE id = $1").bind(item_id).execute(&mut *tx).await?;
    let target = Target::on_item(&activity::item_target(&mut tx, item_id).await?, "comment", id);
    activity::record(&mut tx, user, "created", &target, Changes::new().value("body", &body).take()).await?;
    tx.commit().await?;
    Ok(sqlx::query_as::<_, Comment>(&format!("{COMMENT_SELECT} WHERE c.id = $1")).bind(id).fetch_one(db).await?)
}

/// Adds a relation from `source` to `target`, normalised so each fact is one row.
pub async fn add_relation(db: &PgPool, user: &AuthUser, source: Uuid, target: Uuid, kind: &str) -> ApiResult<()> {
    check_relation(kind)?;
    let (_, a) = access::work_item(db, source, user).await?;
    let (_, b) = access::work_item(db, target, user).await?;
    if a.workspace_id != b.workspace_id {
        return Err(bad("related items must be in the same workspace"));
    }
    if source == target {
        return Err(bad("an item can't relate to itself"));
    }
    let (s, t, k) = match kind {
        "blocked_by" => (target, source, "blocks"),
        "relates_to" if target < source => (target, source, "relates_to"),
        _ => (source, target, kind),
    };
    let mut tx = db.begin().await?;
    let id: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO work_item_relations (source_id, target_id, kind, created_by) VALUES ($1, $2, $3, $4)
         ON CONFLICT (source_id, target_id, kind) DO NOTHING RETURNING id",
    )
    .bind(s)
    .bind(t)
    .bind(k)
    .bind(user.id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(id) = id {
        let other = activity::item_target(&mut tx, target).await?;
        let mut t = Target::on_item(&activity::item_target(&mut tx, source).await?, "relation", id);
        t.related_item_id = Some(target);
        let mut c = Changes::new();
        c.value("kind", kind).value("item", &other.label);
        activity::record(&mut tx, user, "created", &t, c.into_value()).await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn add_link(db: &PgPool, user: &AuthUser, item_id: Uuid, url: &str, title: Option<&str>) -> ApiResult<Link> {
    access::work_item(db, item_id, user).await?;
    let parsed = url::Url::parse(url.trim()).map_err(|_| bad("that isn't a valid URL"))?;
    if !matches!(parsed.scheme(), "http" | "https" | "mailto" | "figma") {
        return Err(bad("links must be http(s) URLs"));
    }
    let title = title.map(str::trim).filter(|t| !t.is_empty());
    let mut tx = db.begin().await?;
    let link = sqlx::query_as::<_, Link>(
        "INSERT INTO work_item_links (work_item_id, url, title, created_by) VALUES ($1, $2, $3, $4)
         RETURNING id, url, title, created_at",
    )
    .bind(item_id)
    .bind(parsed.to_string())
    .bind(title)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    let target = Target::on_item(&activity::item_target(&mut tx, item_id).await?, "link", link.id);
    let mut c = Changes::new();
    c.value("url", &link.url).value("title", &link.title);
    activity::record(&mut tx, user, "created", &target, c.into_value()).await?;
    tx.commit().await?;
    Ok(link)
}

/// Finds an item by uuid, or by key ("MTH-12") within the user's workspaces.
pub async fn resolve(db: &PgPool, user: &AuthUser, id_or_key: &str) -> ApiResult<Uuid> {
    let s = id_or_key.trim();
    if let Ok(id) = Uuid::parse_str(s) {
        access::work_item(db, id, user).await?;
        return Ok(id);
    }
    let (ident, seq) = s
        .rsplit_once('-')
        .and_then(|(i, n)| n.parse::<i32>().ok().map(|n| (i.to_ascii_uppercase(), n)))
        .ok_or_else(|| bad("expected a work item id or key such as MTH-12"))?;
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT w.id FROM work_items w JOIN projects p ON p.id = w.project_id
         JOIN workspace_members m ON m.workspace_id = p.workspace_id AND m.user_id = $3
         WHERE p.identifier = $1 AND w.seq = $2",
    )
    .bind(ident)
    .bind(seq)
    .bind(user.id)
    .fetch_all(db)
    .await?;
    match ids.as_slice() {
        [id] => Ok(*id),
        [] => Err(ApiError::NotFound),
        _ => Err(bad("that key matches items in more than one workspace; use the item's id")),
    }
}
