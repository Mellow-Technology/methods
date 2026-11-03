mod access;
mod activity;
mod auth;
mod config;
mod error;
mod live;
mod mcp;
mod models;
mod richtext;
mod routes;
mod service;
mod storage;

use actix_files::{Files, NamedFile};
use actix_web::{
    App, HttpRequest, HttpServer,
    dev::{ServiceRequest, ServiceResponse, fn_service},
    middleware::{DefaultHeaders, Logger},
    web,
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tracing_subscriber::EnvFilter;

use crate::{config::Config, error::ApiError, routes::oidc::Oidc, storage::Storage};

pub struct AppState {
    pub db: PgPool,
    pub config: Config,
    pub storage: Storage,
    pub oidc: Option<Oidc>,
    pub hub: live::Hub,
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,sqlx=warn".into()))
        .init();

    let config = Config::from_env()?;
    let db = PgPoolOptions::new().max_connections(16).connect(&config.database_url).await?;
    sqlx::migrate!("./migrations").run(&db).await?;
    richtext::convert_legacy(&db).await?;

    let storage = Storage::from_config(&config.storage)?;
    let oidc = config.oidc.clone().map(|o| Oidc::new(o, &config.public_url)).transpose()?;
    tracing::info!(
        bind = %config.bind,
        storage = %storage.description,
        oidc = oidc.is_some(),
        spa = config.static_dir.as_deref().unwrap_or("(not served)"),
        "starting methods"
    );

    let bind = config.bind.clone();
    let hub = live::Hub::start(db.clone());
    let state = web::Data::new(AppState { db, config, storage, oidc, hub });

    HttpServer::new(move || {
        let json = web::JsonConfig::default()
            .limit(2 * 1024 * 1024)
            .error_handler(|err, _req| ApiError::BadRequest(err.to_string()).into());
        let query = web::QueryConfig::default().error_handler(|err, _req| ApiError::BadRequest(err.to_string()).into());
        let path = web::PathConfig::default().error_handler(|_err, _req| ApiError::NotFound.into());

        let mut app = App::new()
            .app_data(state.clone())
            .app_data(json)
            .app_data(query)
            .app_data(path)
            .wrap(Logger::new("%r %s %Dms"))
            .wrap(DefaultHeaders::new().add(("X-Frame-Options", "DENY")).add(("Referrer-Policy", "same-origin")))
            .service(web::scope("/api").configure(routes::configure))
            .service(
                web::resource("/mcp")
                    .route(web::post().to(mcp::handle))
                    .route(web::get().to(mcp::get_not_allowed)),
            );

        // Serve the exported Next.js SPA, falling back to its index for
        // unknown paths so client-side routes work on reload.
        if let Some(dir) = state.config.static_dir.clone() {
            let index = format!("{dir}/index.html");
            app = app.service(
                Files::new("/", dir)
                    .index_file("index.html")
                    .redirect_to_slash_directory()
                    .default_handler(fn_service(move |req: ServiceRequest| {
                        let index = index.clone();
                        async move {
                            let (req, _): (HttpRequest, _) = req.into_parts();
                            let file = NamedFile::open_async(&index).await?;
                            let res = file.into_response(&req);
                            Ok(ServiceResponse::new(req, res))
                        }
                    })),
            );
        }
        app
    })
    .bind(&bind)?
    .run()
    .await?;
    Ok(())
}
