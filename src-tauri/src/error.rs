use serde::{Serialize, Serializer};

/// Typed error surface for Tauri commands.
///
/// Implements Serialize so the frontend receives a structured error string
/// (Tauri commands return Result<T, E> where E: Serialize).
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("repository not configured or missing on disk: {0}")]
    RepoMissing(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("yaml parse error in {path}: {source}")]
    YamlParse {
        path: String,
        #[source]
        source: serde_yaml::Error,
    },

    #[error("yaml encode error: {0}")]
    YamlEncode(#[from] serde_yaml::Error),

    #[error("schema validation failed for {path}: {message}")]
    SchemaValidation { path: String, message: String },

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("git error: {0}")]
    Git(String),

    #[error("other: {0}")]
    Other(String),
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(&self.to_string())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Other(format!("{e:#}"))
    }
}

pub type AppResult<T> = Result<T, AppError>;
