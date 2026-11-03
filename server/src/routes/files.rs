use actix_multipart::Multipart;
use actix_web::{
    HttpResponse,
    http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS},
    web,
};
use bytes::BytesMut;
use chrono::Datelike;
use futures_util::TryStreamExt;
use uuid::Uuid;

use crate::{
    AppState,
    auth::AuthUser,
    error::{ApiError, ApiResult, bad},
    models::FileOut,
};

const FILE_SELECT: &str = "SELECT id, '/api/files/' || id::text AS url, file_name, content_type, size_bytes, created_at FROM files";

/// Reads the first file part of a multipart upload into object storage.
pub async fn store_upload(state: &AppState, user: &AuthUser, mut payload: Multipart) -> ApiResult<FileOut> {
    let max = state.config.max_upload_bytes;
    while let Some(mut field) = payload.try_next().await.map_err(|e| bad(format!("bad upload: {e}")))? {
        let Some(name) = field.content_disposition().and_then(|cd| cd.get_filename()).map(str::to_string) else {
            continue;
        };
        let content_type = field
            .content_type()
            .map(|m| m.essence_str().to_string())
            .unwrap_or_else(|| "application/octet-stream".into());
        let mut data = BytesMut::new();
        while let Some(chunk) = field.try_next().await.map_err(|e| bad(format!("bad upload: {e}")))? {
            if data.len() + chunk.len() > max {
                return Err(bad(format!("files can be at most {} MB", max / 1024 / 1024)));
            }
            data.extend_from_slice(&chunk);
        }
        let file_name: String = name
            .chars()
            .filter(|c| !c.is_control() && !matches!(c, '/' | '\\'))
            .take(200)
            .collect();
        let file_name = if file_name.trim().is_empty() { "file".to_string() } else { file_name };
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();
        let key = format!("files/{:04}/{:02}/{id}", now.year(), now.month());
        let size = data.len() as i64;
        state.storage.put(&key, data.freeze()).await?;
        let file = sqlx::query_as::<_, FileOut>(
            "INSERT INTO files (id, object_key, file_name, content_type, size_bytes, uploaded_by)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING id, '/api/files/' || id::text AS url, file_name, content_type, size_bytes, created_at",
        )
        .bind(id)
        .bind(&key)
        .bind(file_name)
        .bind(content_type)
        .bind(size)
        .bind(user.id)
        .fetch_one(&state.db)
        .await?;
        return Ok(file);
    }
    Err(bad("no file in upload"))
}

pub async fn delete_file(state: &AppState, id: Uuid) -> ApiResult<()> {
    let key: Option<String> = sqlx::query_scalar("DELETE FROM files WHERE id = $1 RETURNING object_key")
        .bind(id)
        .fetch_optional(&state.db)
        .await?;
    if let Some(key) = key {
        if let Err(e) = state.storage.delete(&key).await {
            tracing::warn!(error = %e, key, "couldn't delete object");
        }
    }
    Ok(())
}

pub async fn upload(state: web::Data<AppState>, user: AuthUser, payload: Multipart) -> ApiResult<HttpResponse> {
    Ok(HttpResponse::Created().json(store_upload(&state, &user, payload).await?))
}

pub async fn info(state: web::Data<AppState>, _user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    let f = sqlx::query_as::<_, FileOut>(&format!("{FILE_SELECT} WHERE id = $1"))
        .bind(*path)
        .fetch_one(&state.db)
        .await?;
    Ok(HttpResponse::Ok().json(f))
}

/// Streams a file. Any signed-in user may read a file by its (unguessable) id.
pub async fn download(state: web::Data<AppState>, _user: AuthUser, path: web::Path<Uuid>) -> ApiResult<HttpResponse> {
    let row: Option<(String, String, String)> =
        sqlx::query_as("SELECT object_key, file_name, content_type FROM files WHERE id = $1")
            .bind(*path)
            .fetch_optional(&state.db)
            .await?;
    let (key, name, content_type) = row.ok_or(ApiError::NotFound)?;
    let data = state.storage.get(&key).await?;
    // Images render inline; anything else downloads, so uploaded HTML/SVG
    // can't run script on our origin.
    let inline = matches!(
        content_type.as_str(),
        "image/png" | "image/jpeg" | "image/gif" | "image/webp" | "image/avif" | "application/pdf"
    );
    let safe_name: String = name.chars().filter(|c| c.is_ascii_graphic() || *c == ' ').filter(|c| *c != '"').collect();
    let disposition = format!(
        "{}; filename=\"{}\"; filename*=UTF-8''{}",
        if inline { "inline" } else { "attachment" },
        safe_name,
        url::form_urlencoded::byte_serialize(name.as_bytes()).collect::<String>()
    );
    Ok(HttpResponse::Ok()
        .insert_header((CONTENT_TYPE, content_type))
        .insert_header((CONTENT_DISPOSITION, disposition))
        .insert_header((CACHE_CONTROL, "private, max-age=31536000, immutable"))
        .insert_header((X_CONTENT_TYPE_OPTIONS, "nosniff"))
        .body(data))
}
