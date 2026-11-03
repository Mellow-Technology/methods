use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("not found")]
    NotFound,
    #[error("authentication required")]
    Unauthorized,
    #[error("you don't have permission to do that")]
    Forbidden,
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Conflict(String),
    #[error("database error")]
    Db(sqlx::Error),
    #[error("internal error")]
    Other(#[from] anyhow::Error),
}

pub type ApiResult<T> = Result<T, ApiError>;

pub fn bad(msg: impl Into<String>) -> ApiError {
    ApiError::BadRequest(msg.into())
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::RowNotFound => ApiError::NotFound,
            sqlx::Error::Database(db) => match db.code().as_deref() {
                // unique_violation
                Some("23505") => ApiError::Conflict(match db.constraint() {
                    Some(c) if c.contains("identifier") => "that project ID is already in use".into(),
                    Some(c) if c.contains("email") => "that email address is already registered".into(),
                    Some(c) if c.contains("labels") => "a label with that name already exists".into(),
                    Some(c) if c.contains("slug") => "that workspace slug is taken".into(),
                    _ => "that already exists".into(),
                }),
                // foreign_key_violation
                Some("23503") => ApiError::BadRequest("a referenced record does not exist".into()),
                // check_violation
                Some("23514") => ApiError::BadRequest(match db.constraint() {
                    Some(c) if c.contains("identifier") => {
                        "project ID must be 2-10 capital letters or digits, starting with a letter".into()
                    }
                    Some(c) if c.contains("due_date") => "due date can't be before the start date".into(),
                    _ => format!("invalid value ({})", db.constraint().unwrap_or("check")),
                }),
                _ => ApiError::Db(err),
            },
            _ => ApiError::Db(err),
        }
    }
}

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden => StatusCode::FORBIDDEN,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::Db(_) | ApiError::Other(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        match self {
            ApiError::Db(e) => tracing::error!(error = %e, "database error"),
            ApiError::Other(e) => tracing::error!(error = ?e, "internal error"),
            _ => {}
        }
        HttpResponse::build(self.status_code()).json(json!({ "error": self.to_string() }))
    }
}
