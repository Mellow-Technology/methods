use std::env;

#[derive(Clone, Debug)]
pub enum StorageConfig {
    /// Files on local disk. The default for development.
    Local { root: String },
    /// Any S3-compatible service (AWS, MinIO, Garage, R2, ...).
    S3 {
        bucket: String,
        region: String,
        endpoint: Option<String>,
        access_key_id: Option<String>,
        secret_access_key: Option<String>,
        allow_http: bool,
        virtual_hosted: bool,
    },
}

#[derive(Clone, Debug)]
pub struct OidcConfig {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    /// Shown on the login button, e.g. "Sign in with Authentik".
    pub display_name: String,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub bind: String,
    /// The origin people use in the browser. OIDC redirects are built from it.
    pub public_url: String,
    pub password_login: bool,
    pub allow_registration: bool,
    pub session_days: i64,
    pub cookie_secure: bool,
    pub static_dir: Option<String>,
    pub max_upload_bytes: usize,
    pub storage: StorageConfig,
    pub oidc: Option<OidcConfig>,
}

fn var(name: &str) -> Option<String> {
    env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn flag(name: &str, default: bool) -> bool {
    match var(name) {
        Some(v) => matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        None => default,
    }
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let public_url = var("PUBLIC_URL")
            .unwrap_or_else(|| "http://localhost:3000".into())
            .trim_end_matches('/')
            .to_string();

        let storage = match var("S3_BUCKET") {
            Some(bucket) => StorageConfig::S3 {
                bucket,
                region: var("S3_REGION").unwrap_or_else(|| "us-east-1".into()),
                endpoint: var("S3_ENDPOINT"),
                access_key_id: var("S3_ACCESS_KEY_ID"),
                secret_access_key: var("S3_SECRET_ACCESS_KEY"),
                allow_http: flag("S3_ALLOW_HTTP", false),
                virtual_hosted: flag("S3_VIRTUAL_HOSTED", false),
            },
            None => StorageConfig::Local {
                root: var("LOCAL_STORAGE_DIR").unwrap_or_else(|| "./data/files".into()),
            },
        };

        let oidc = match (var("OIDC_ISSUER"), var("OIDC_CLIENT_ID")) {
            (Some(issuer), Some(client_id)) => Some(OidcConfig {
                issuer,
                client_id,
                client_secret: var("OIDC_CLIENT_SECRET"),
                display_name: var("OIDC_NAME").unwrap_or_else(|| "Single sign-on".into()),
            }),
            _ => None,
        };

        let static_dir = var("STATIC_DIR").or_else(|| {
            let default = "../web/out";
            std::path::Path::new(default).is_dir().then(|| default.to_string())
        });

        Ok(Config {
            database_url: var("DATABASE_URL").unwrap_or_else(|| "postgres://localhost/methods".into()),
            bind: var("BIND_ADDR").unwrap_or_else(|| "127.0.0.1:8080".into()),
            cookie_secure: flag("COOKIE_SECURE", public_url.starts_with("https://")),
            public_url,
            password_login: flag("PASSWORD_LOGIN", true),
            allow_registration: flag("ALLOW_REGISTRATION", true),
            session_days: var("SESSION_DAYS").and_then(|v| v.parse().ok()).unwrap_or(30),
            static_dir,
            max_upload_bytes: var("MAX_UPLOAD_MB")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(25)
                * 1024
                * 1024,
            storage,
            oidc,
        })
    }
}
