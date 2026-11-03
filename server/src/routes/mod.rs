pub mod auth;
pub mod files;
pub mod oidc;
pub mod projects;
pub mod work_items;
pub mod workspaces;

use actix_web::{HttpResponse, web};

/// Everything under `/api`.
pub fn configure(cfg: &mut web::ServiceConfig) {
    use web::{delete, get, patch, post, resource as r};
    cfg.route("/health", get().to(|| async { HttpResponse::Ok().json(serde_json::json!({ "ok": true })) }))
        // Auth & profile
        .route("/auth/config", get().to(auth::config))
        .route("/auth/register", post().to(auth::register))
        .route("/auth/login", post().to(auth::login))
        .route("/auth/logout", post().to(auth::logout))
        .route("/auth/oidc/start", get().to(oidc::start))
        .route("/auth/oidc/callback", get().to(oidc::callback))
        .service(r("/me").route(get().to(auth::me)).route(patch().to(auth::update_me)))
        // Workspaces, members, bots
        .service(r("/workspaces").route(get().to(workspaces::list)).route(post().to(workspaces::create)))
        .service(r("/workspaces/{id}").route(get().to(workspaces::get)).route(patch().to(workspaces::update)))
        .service(
            r("/workspaces/{id}/members")
                .route(get().to(workspaces::members))
                .route(post().to(workspaces::add_member)),
        )
        .service(
            r("/workspaces/{id}/members/{user_id}")
                .route(patch().to(workspaces::update_member))
                .route(delete().to(workspaces::remove_member)),
        )
        .service(
            r("/workspaces/{id}/bots").route(get().to(workspaces::list_bots)).route(post().to(workspaces::create_bot)),
        )
        .service(r("/bots/{id}").route(patch().to(workspaces::update_bot)).route(delete().to(workspaces::delete_bot)))
        .service(
            r("/bots/{id}/tokens").route(get().to(workspaces::list_tokens)).route(post().to(workspaces::create_token)),
        )
        .route("/bot-tokens/{id}", delete().to(workspaces::revoke_token))
        // Projects
        .service(
            r("/workspaces/{id}/projects").route(get().to(projects::list)).route(post().to(projects::create)),
        )
        .service(
            r("/projects/{id}")
                .route(get().to(projects::get))
                .route(patch().to(projects::update))
                .route(delete().to(projects::delete)),
        )
        .service(
            r("/projects/{id}/labels").route(get().to(projects::list_labels)).route(post().to(projects::create_label)),
        )
        .service(r("/labels/{id}").route(patch().to(projects::update_label)).route(delete().to(projects::delete_label)))
        .service(
            r("/projects/{id}/epics").route(get().to(projects::list_epics)).route(post().to(projects::create_epic)),
        )
        .service(r("/epics/{id}").route(patch().to(projects::update_epic)).route(delete().to(projects::delete_epic)))
        .service(
            r("/projects/{id}/work-items").route(get().to(work_items::list)).route(post().to(work_items::create)),
        )
        // Work items
        .service(
            r("/work-items/{id}")
                .route(get().to(work_items::get))
                .route(patch().to(work_items::update))
                .route(delete().to(work_items::delete)),
        )
        .service(
            r("/work-items/{id}/comments")
                .route(get().to(work_items::list_comments))
                .route(post().to(work_items::add_comment)),
        )
        .service(
            r("/comments/{id}")
                .route(patch().to(work_items::update_comment))
                .route(delete().to(work_items::delete_comment)),
        )
        .route("/work-items/{id}/relations", post().to(work_items::add_relation))
        .route("/relations/{id}", delete().to(work_items::delete_relation))
        .route("/work-items/{id}/links", post().to(work_items::add_link))
        .route("/links/{id}", delete().to(work_items::delete_link))
        .route("/work-items/{id}/reactions", post().to(work_items::toggle_reaction))
        .route("/work-items/{id}/attachments", post().to(work_items::add_attachment))
        .route("/attachments/{id}", delete().to(work_items::delete_attachment))
        // Files
        .route("/work-items/{id}/activity", get().to(work_items::item_activity))
        .route("/work-items/{id}/description-history", get().to(work_items::description_history))
        .route("/projects/{id}/activity", get().to(work_items::project_activity))
        .route("/workspaces/{id}/activity", get().to(work_items::workspace_activity))
        .route("/workspaces/{id}/events", get().to(crate::live::events))
        .route("/activity/{id}/revert", post().to(work_items::revert))
        .route("/files", post().to(files::upload))
        .route("/files/{id}", get().to(files::download))
        .route("/files/{id}/info", get().to(files::info));
}
