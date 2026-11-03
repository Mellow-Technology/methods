//! Model Context Protocol endpoint for bot users (Streamable HTTP transport,
//! stateless JSON responses). Bots authenticate with
//! `Authorization: Bearer mth_...`; tools run with the bot's own permissions.

use actix_web::{HttpResponse, http::header, web};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    activity,
    richtext,
    AppState, access,
    auth::AuthUser,
    error::{ApiError, ApiResult, bad},
    models::{Comment, Member, Project, PROJECT_COLUMNS, WorkItemDetail, Workspace},
    routes::projects::{
        CreateProject, EpicBody, LabelBody, create_epic_for, create_label_for, create_project_for, list_epics_for,
        list_labels_for, update_epic_for,
    },
    service::{self, CreateWorkItem, UpdateWorkItem, WorkItemFilter},
};

const SUPPORTED_VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Methods is a project management system. Workspaces contain projects; projects \
contain work items shown on a kanban board with the states backlog, todo, in_progress, done and cancelled. \
Work items are referred to by key (e.g. MTH-12) or id. Start with list_projects to find a project, then \
list_work_items. Assign work to yourself with update_work_item and the id from whoami. Descriptions and \
comments are Markdown, both when you write them and when you read them.";

/// Bots read rich text as Markdown rather than as a document tree.
fn item_for_bot(item: WorkItemDetail) -> Value {
    let mut v = serde_json::to_value(item).unwrap_or_default();
    let md = v.get("description").filter(|d| d.is_object()).map(richtext::to_markdown);
    v["description"] = md.map_or(Value::Null, Value::String);
    v
}

fn comments_for_bot(comments: Vec<Comment>) -> Vec<Value> {
    comments
        .into_iter()
        .map(|c| {
            let mut v = serde_json::to_value(c).unwrap_or_default();
            v["body"] = Value::String(richtext::to_markdown(&v["body"]));
            v
        })
        .collect()
}

pub async fn get_not_allowed() -> HttpResponse {
    // We don't offer a server-initiated SSE stream.
    HttpResponse::MethodNotAllowed().insert_header((header::ALLOW, "POST")).finish()
}

pub async fn handle(state: web::Data<AppState>, user: AuthUser, body: web::Json<Value>) -> HttpResponse {
    let body = body.into_inner();
    let responses: Vec<Value> = match body {
        Value::Array(batch) => {
            let mut out = Vec::new();
            for msg in batch {
                if let Some(r) = dispatch(&state, &user, msg).await {
                    out.push(r);
                }
            }
            out
        }
        msg => dispatch(&state, &user, msg).await.into_iter().collect(),
    };
    match responses.len() {
        0 => HttpResponse::Accepted().finish(),
        1 => HttpResponse::Ok().json(&responses[0]),
        _ => HttpResponse::Ok().json(responses),
    }
}

fn rpc_error(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message.into() } })
}

async fn dispatch(state: &AppState, user: &AuthUser, msg: Value) -> Option<Value> {
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("").to_string();
    // Notifications (no id) and responses get no reply.
    let id = msg.get("id").cloned()?;
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let result = match method.as_str() {
        "initialize" => {
            let requested = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
            let version = SUPPORTED_VERSIONS.iter().find(|v| **v == requested).unwrap_or(&SUPPORTED_VERSIONS[0]);
            json!({
                "protocolVersion": version,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "methods", "title": "Methods", "version": env!("CARGO_PKG_VERSION") },
                "instructions": INSTRUCTIONS,
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            match call_tool(state, user, name, args).await {
                Ok(value) => json!({
                    "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_default() }],
                    "structuredContent": if value.is_object() { value.clone() } else { json!({ "result": value }) },
                    "isError": false,
                }),
                Err(ToolError::Unknown) => return Some(rpc_error(id, -32602, format!("unknown tool: {name}"))),
                Err(ToolError::Api(e)) => json!({
                    "content": [{ "type": "text", "text": format!("Error: {e}") }],
                    "isError": true,
                }),
            }
        }
        "resources/list" => json!({ "resources": [] }),
        "prompts/list" => json!({ "prompts": [] }),
        _ => return Some(rpc_error(id, -32601, format!("method not found: {method}"))),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

enum ToolError {
    Unknown,
    Api(ApiError),
}

impl<E: Into<ApiError>> From<E> for ToolError {
    fn from(e: E) -> Self {
        ToolError::Api(e.into())
    }
}

fn args<T: for<'de> Deserialize<'de>>(v: Value) -> Result<T, ToolError> {
    serde_json::from_value(v).map_err(|e| ToolError::Api(bad(format!("invalid arguments: {e}"))))
}

/// A project given by id or by its identifier (e.g. "MTH").
async fn resolve_project(state: &AppState, user: &AuthUser, project: &str) -> ApiResult<Uuid> {
    if let Ok(id) = Uuid::parse_str(project.trim()) {
        access::project(&state.db, id, user).await?;
        return Ok(id);
    }
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT p.id FROM projects p JOIN workspace_members m ON m.workspace_id = p.workspace_id AND m.user_id = $2
         WHERE p.identifier = upper($1)",
    )
    .bind(project.trim())
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;
    match ids.as_slice() {
        [id] => Ok(*id),
        [] => Err(ApiError::NotFound),
        _ => Err(bad("several projects use that identifier; pass the project's id")),
    }
}

#[derive(Deserialize)]
struct ProjectArg {
    project: String,
}

#[derive(Deserialize)]
struct ItemArg {
    work_item: String,
}

async fn call_tool(state: &AppState, user: &AuthUser, name: &str, a: Value) -> Result<Value, ToolError> {
    let db = &state.db;
    Ok(match name {
        "whoami" => json!({ "id": user.id, "name": user.display_name, "kind": user.kind }),
        "list_workspaces" => {
            let rows = sqlx::query_as::<_, Workspace>(
                "SELECT w.id, w.name, w.slug, m.role,
                        (SELECT count(*) FROM projects p WHERE p.workspace_id = w.id) AS project_count, w.created_at
                 FROM workspaces w JOIN workspace_members m ON m.workspace_id = w.id AND m.user_id = $1
                 ORDER BY w.name",
            )
            .bind(user.id)
            .fetch_all(db)
            .await?;
            json!({ "workspaces": rows })
        }
        "list_projects" => {
            #[derive(Deserialize)]
            struct A {
                workspace_id: Option<Uuid>,
            }
            let a: A = args(a)?;
            let rows = sqlx::query_as::<_, Project>(&format!(
                "SELECT {PROJECT_COLUMNS} FROM projects p
                 JOIN workspace_members m ON m.workspace_id = p.workspace_id AND m.user_id = $1
                 WHERE ($2::uuid IS NULL OR p.workspace_id = $2) ORDER BY p.name"
            ))
            .bind(user.id)
            .bind(a.workspace_id)
            .fetch_all(db)
            .await?;
            json!({ "projects": rows })
        }
        "create_project" => {
            #[derive(Deserialize)]
            struct A {
                workspace_id: Option<Uuid>,
                name: String,
                identifier: String,
                description: Option<String>,
                icon_emoji: Option<String>,
            }
            let a: A = args(a)?;
            let workspace_id = match a.workspace_id {
                Some(id) => id,
                None => {
                    let ids: Vec<Uuid> =
                        sqlx::query_scalar("SELECT workspace_id FROM workspace_members WHERE user_id = $1")
                            .bind(user.id)
                            .fetch_all(db)
                            .await?;
                    match ids.as_slice() {
                        [id] => *id,
                        [] => return Err(ApiError::NotFound.into()),
                        _ => return Err(bad("you belong to several workspaces; pass workspace_id").into()),
                    }
                }
            };
            let body = CreateProject {
                name: a.name,
                identifier: a.identifier,
                description: a.description.unwrap_or_default(),
                icon_emoji: a.icon_emoji,
                icon_file_id: None,
                cover_file_id: None,
            };
            json!({ "project": create_project_for(db, user, workspace_id, body).await? })
        }
        "get_project" => {
            let a: ProjectArg = args(a)?;
            let id = resolve_project(state, user, &a.project).await?;
            let project = crate::routes::projects::project_out(db, id).await?;
            let members = sqlx::query_as::<_, Member>(
                "SELECT u.id, u.kind, u.display_name, u.email, '/api/files/' || u.avatar_file_id::text AS avatar_url, m.role
                 FROM workspace_members m JOIN users u ON u.id = m.user_id WHERE m.workspace_id = $1
                 ORDER BY u.display_name",
            )
            .bind(project.workspace_id)
            .fetch_all(db)
            .await?;
            json!({
                "project": project,
                "labels": list_labels_for(db, id).await?,
                "epics": list_epics_for(db, id).await?,
                "members": members,
            })
        }
        "list_work_items" => {
            #[derive(Deserialize)]
            struct A {
                project: String,
                #[serde(flatten)]
                filter: WorkItemFilter,
            }
            let a: A = args(a)?;
            let id = resolve_project(state, user, &a.project).await?;
            let mut filter = a.filter;
            filter.limit = Some(filter.limit.unwrap_or(200));
            json!({ "work_items": service::list(db, id, &filter).await? })
        }
        "get_work_item" => {
            let a: ItemArg = args(a)?;
            let id = service::resolve(db, user, &a.work_item).await?;
            let detail = service::detail(db, id, user).await?;
            let comments = service::list_comments(db, user, id).await?;
            json!({ "work_item": item_for_bot(detail), "comments": comments_for_bot(comments) })
        }
        "create_work_item" => {
            #[derive(Deserialize)]
            struct A {
                project: String,
                #[serde(flatten)]
                item: CreateWorkItem,
            }
            let a: A = args(a)?;
            let pid = resolve_project(state, user, &a.project).await?;
            json!({ "work_item": item_for_bot(service::create(db, user, pid, a.item).await?) })
        }
        "update_work_item" => {
            #[derive(Deserialize)]
            struct A {
                work_item: String,
                #[serde(flatten)]
                changes: UpdateWorkItem,
            }
            let a: A = args(a)?;
            let id = service::resolve(db, user, &a.work_item).await?;
            json!({ "work_item": item_for_bot(service::update(db, user, id, a.changes).await?) })
        }
        "add_comment" => {
            #[derive(Deserialize)]
            struct A {
                work_item: String,
                body: String,
            }
            let a: A = args(a)?;
            let id = service::resolve(db, user, &a.work_item).await?;
            let comment = service::add_comment(db, user, id, &Value::String(a.body)).await?;
            json!({ "comment": comments_for_bot(vec![comment]).pop() })
        }
        "add_relation" => {
            #[derive(Deserialize)]
            struct A {
                work_item: String,
                kind: String,
                target: String,
            }
            let a: A = args(a)?;
            let source = service::resolve(db, user, &a.work_item).await?;
            let target = service::resolve(db, user, &a.target).await?;
            service::add_relation(db, user, source, target, &a.kind).await?;
            json!({ "relations": service::detail(db, source, user).await?.relations })
        }
        "add_link" => {
            #[derive(Deserialize)]
            struct A {
                work_item: String,
                url: String,
                title: Option<String>,
            }
            let a: A = args(a)?;
            let id = service::resolve(db, user, &a.work_item).await?;
            json!({ "link": service::add_link(db, user, id, &a.url, a.title.as_deref()).await? })
        }
        "create_epic" => {
            #[derive(Deserialize)]
            struct A {
                project: String,
                name: String,
                description: Option<String>,
            }
            let a: A = args(a)?;
            let pid = resolve_project(state, user, &a.project).await?;
            let body = EpicBody { name: Some(a.name), description: a.description, color: None };
            json!({ "epic": create_epic_for(db, user, pid, &body).await? })
        }
        "update_epic" => {
            #[derive(Deserialize)]
            struct A {
                epic_id: Uuid,
                #[serde(flatten)]
                body: EpicBody,
            }
            let a: A = args(a)?;
            json!({ "epic": update_epic_for(db, user, a.epic_id, &a.body).await? })
        }
        "create_label" => {
            #[derive(Deserialize)]
            struct A {
                project: String,
                #[serde(flatten)]
                body: LabelBody,
            }
            let a: A = args(a)?;
            let pid = resolve_project(state, user, &a.project).await?;
            json!({ "label": create_label_for(db, user, pid, &a.body).await? })
        }
        "delete_work_item" => {
            let a: ItemArg = args(a)?;
            let id = service::resolve(db, user, &a.work_item).await?;
            service::delete(db, user, id).await?;
            json!({ "deleted": id })
        }
        "get_activity" => {
            #[derive(Deserialize)]
            struct A {
                work_item: Option<String>,
                project: Option<String>,
                before: Option<i64>,
                limit: Option<i64>,
            }
            let a: A = args(a)?;
            let scope = match (a.work_item, a.project) {
                (Some(item), _) => activity::Scope::WorkItem(service::resolve(db, user, &item).await?),
                (None, Some(p)) => activity::Scope::Project(resolve_project(state, user, &p).await?),
                (None, None) => return Err(ApiError::BadRequest("pass work_item or project".into()).into()),
            };
            let page = activity::Page { before: a.before, limit: Some(a.limit.unwrap_or(30)) };
            json!({ "activity": activity::list(db, scope, &page).await? })
        }
        _ => return Err(ToolError::Unknown),
    })
}

fn tools() -> Value {
    let project = json!({ "type": "string", "description": "Project id, or its identifier such as \"MTH\"" });
    let item = json!({ "type": "string", "description": "Work item id, or its key such as \"MTH-12\"" });
    let state = json!({ "type": "string", "enum": ["backlog", "todo", "in_progress", "done", "cancelled"] });
    let priority = json!({ "type": "string", "enum": ["urgent", "high", "medium", "low"] });
    let uuid = |d: &str| json!({ "type": ["string", "null"], "format": "uuid", "description": d });
    let date = |d: &str| json!({ "type": ["string", "null"], "format": "date", "description": d });
    let fields = json!({
        "title": { "type": "string" },
        "description": { "type": ["string", "null"], "description": "Markdown; null clears it" },
        "state": state,
        "priority": priority,
        "assignee_id": uuid("User id of a workspace member (human or bot); null to unassign"),
        "start_date": date("YYYY-MM-DD"),
        "due_date": date("YYYY-MM-DD"),
        "parent_id": uuid("Parent work item id"),
        "epic_id": uuid("Epic id"),
        "label_ids": { "type": "array", "items": { "type": "string", "format": "uuid" } },
    });
    let with = |extra: Value, base: &Value| {
        let mut m = base.as_object().cloned().unwrap_or_default();
        m.extend(extra.as_object().cloned().unwrap_or_default());
        Value::Object(m)
    };
    let read_only = json!({ "readOnlyHint": true });
    let color = json!({ "type": "string", "description": "Hex colour such as #6366f1" });

    json!([
        { "name": "whoami", "description": "Your own user id and name.",
          "inputSchema": { "type": "object", "properties": {} }, "annotations": read_only },
        { "name": "list_workspaces", "description": "Workspaces you belong to.",
          "inputSchema": { "type": "object", "properties": {} }, "annotations": read_only },
        { "name": "list_projects", "description": "Projects you can see, optionally in one workspace.",
          "inputSchema": { "type": "object", "properties": { "workspace_id": uuid("Workspace id") } },
          "annotations": read_only },
        { "name": "create_project",
          "description": "Create a project in a workspace. workspace_id may be left out when you belong to only one workspace.",
          "inputSchema": { "type": "object", "required": ["name", "identifier"], "properties": {
              "workspace_id": uuid("Workspace id"),
              "name": { "type": "string" },
              "identifier": { "type": "string", "description": "Short key prefix for work items, 2-10 letters or digits starting with a letter, e.g. \"MTH\"" },
              "description": { "type": "string" },
              "icon_emoji": { "type": "string", "description": "A single emoji" } } } },
        { "name": "get_project",
          "description": "A project with its labels, epics and the members work can be assigned to.",
          "inputSchema": { "type": "object", "properties": { "project": project }, "required": ["project"] },
          "annotations": read_only },
        { "name": "list_work_items", "description": "Work items in a project, optionally filtered.",
          "inputSchema": { "type": "object", "required": ["project"], "properties": {
              "project": project, "state": state, "assignee_id": uuid("Only items assigned to this user"),
              "epic_id": uuid("Only items in this epic"), "label_id": uuid("Only items with this label"),
              "q": { "type": "string", "description": "Search titles and keys" },
              "limit": { "type": "integer", "minimum": 1, "maximum": 1000 } } },
          "annotations": read_only },
        { "name": "get_work_item",
          "description": "Everything about a work item: fields, labels, relations, links, attachments, comments.",
          "inputSchema": { "type": "object", "properties": { "work_item": item }, "required": ["work_item"] },
          "annotations": read_only },
        { "name": "create_work_item", "description": "Create a work item. You're recorded as its creator.",
          "inputSchema": { "type": "object", "required": ["project", "title"],
                           "properties": with(json!({ "project": project }), &fields) } },
        { "name": "update_work_item",
          "description": "Change a work item's fields: move it between states, reassign it, set dates and so on. Only the fields you pass change.",
          "inputSchema": { "type": "object", "required": ["work_item"],
                           "properties": with(json!({ "work_item": item }), &fields) } },
        { "name": "add_comment", "description": "Comment on a work item.",
          "inputSchema": { "type": "object", "required": ["work_item", "body"], "properties": {
              "work_item": item, "body": { "type": "string", "description": "Markdown" } } } },
        { "name": "add_relation", "description": "Relate two work items.",
          "inputSchema": { "type": "object", "required": ["work_item", "kind", "target"], "properties": {
              "work_item": item,
              "kind": { "type": "string", "enum": ["relates_to", "duplicate_of", "blocked_by", "blocks"] },
              "target": item } } },
        { "name": "add_link", "description": "Attach a URL to a work item.",
          "inputSchema": { "type": "object", "required": ["work_item", "url"], "properties": {
              "work_item": item, "url": { "type": "string", "format": "uri" }, "title": { "type": "string" } } } },
        { "name": "create_epic", "description": "Create an epic to group work items in a project.",
          "inputSchema": { "type": "object", "required": ["project", "name"], "properties": {
              "project": project, "name": { "type": "string" }, "description": { "type": "string" } } } },
        { "name": "update_epic", "description": "Rename an epic or change its description or colour. Only the fields you pass change.",
          "inputSchema": { "type": "object", "required": ["epic_id"], "properties": {
              "epic_id": { "type": "string", "format": "uuid" }, "name": { "type": "string" },
              "description": { "type": "string" }, "color": color.clone() } } },
        { "name": "create_label", "description": "Create a label in a project.",
          "inputSchema": { "type": "object", "required": ["project", "name"], "properties": {
              "project": project, "name": { "type": "string" }, "color": color } } },
        { "name": "delete_work_item",
          "description": "Move a work item to the trash. It can be restored by undoing the deletion in its history.",
          "annotations": { "destructiveHint": true },
          "inputSchema": { "type": "object", "required": ["work_item"], "properties": { "work_item": item } } },
        { "name": "get_activity",
          "description": "History of a work item or project, newest first: who changed what, with old and new values. Page with `before` (an entry id).",
          "annotations": read_only,
          "inputSchema": { "type": "object", "properties": {
              "work_item": item, "project": project,
              "before": { "type": "integer" }, "limit": { "type": "integer", "minimum": 1, "maximum": 200 } } } },
    ])
}
