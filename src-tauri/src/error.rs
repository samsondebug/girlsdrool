//! The error taxonomy shared by every command (ARCHITECTURE §7).
//!
//! Over IPC an error serializes as `{ kind, message, detail }`. `kind` is stable and the webview
//! switches on it; `message` is for people; `detail` carries the structured fields a screen needs
//! (row and column for a parse error, the field for a validation error, …).

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("the database is locked")]
    Locked,

    #[error("wrong passphrase")]
    WrongPassphrase,

    #[error("database error: {0}")]
    Db(String),

    #[error("migration error: {0}")]
    Migration(String),

    #[error("i/o error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("row {row}, column {column}: {message}")]
    Parse {
        row: usize,
        column: String,
        message: String,
    },

    #[error("unsupported: {0}")]
    Unsupported(String),

    #[error("{field}: {message}")]
    Validation { field: String, message: String },

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("{entity} {id} not found")]
    NotFound { entity: &'static str, id: i64 },

    #[error("blocked by policy {policy}: {message}")]
    PolicyBlocked { policy: String, message: String },

    #[error("money arithmetic overflowed")]
    Overflow,

    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    /// Stable discriminator used by the webview.
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Locked => "Locked",
            AppError::WrongPassphrase => "WrongPassphrase",
            AppError::Db(_) => "Db",
            AppError::Migration(_) => "Migration",
            AppError::Io { .. } => "Io",
            AppError::Parse { .. } => "Parse",
            AppError::Unsupported(_) => "Unsupported",
            AppError::Validation { .. } => "Validation",
            AppError::Conflict(_) => "Conflict",
            AppError::NotFound { .. } => "NotFound",
            AppError::PolicyBlocked { .. } => "PolicyBlocked",
            AppError::Overflow => "Overflow",
            AppError::Internal(_) => "Internal",
        }
    }

    pub fn validation(field: impl Into<String>, message: impl Into<String>) -> Self {
        AppError::Validation {
            field: field.into(),
            message: message.into(),
        }
    }

    pub fn io(path: impl AsRef<std::path::Path>, source: std::io::Error) -> Self {
        AppError::Io {
            path: path.as_ref().display().to_string(),
            source,
        }
    }

    fn detail(&self) -> Option<serde_json::Value> {
        match self {
            AppError::Io { path, .. } => Some(serde_json::json!({ "path": path })),
            AppError::Parse { row, column, .. } => {
                Some(serde_json::json!({ "row": row, "column": column }))
            }
            AppError::Validation { field, .. } => Some(serde_json::json!({ "field": field })),
            AppError::NotFound { entity, id } => {
                Some(serde_json::json!({ "entity": entity, "id": id }))
            }
            AppError::PolicyBlocked { policy, .. } => Some(serde_json::json!({ "policy": policy })),
            _ => None,
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("AppError", 3)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.serialize_field("detail", &self.detail())?;
        s.end()
    }
}

impl From<rusqlite::Error> for AppError {
    /// SQLite messages never contain bound values, so they are safe to surface. A
    /// `NotADatabase` code during open is mapped to `WrongPassphrase` by the open path
    /// itself; everywhere else it is a real database error.
    fn from(e: rusqlite::Error) -> Self {
        AppError::Db(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Internal(format!("json: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_kind_message_detail() {
        let e = AppError::Parse {
            row: 7,
            column: "Amount".into(),
            message: "not a decimal".into(),
        };
        let v = serde_json::to_value(&e).expect("serialize");
        assert_eq!(v["kind"], "Parse");
        assert_eq!(v["message"], "row 7, column Amount: not a decimal");
        assert_eq!(v["detail"]["row"], 7);
        assert_eq!(v["detail"]["column"], "Amount");
    }

    #[test]
    fn detail_is_null_when_there_is_nothing_structured() {
        let v = serde_json::to_value(AppError::Overflow).expect("serialize");
        assert_eq!(v["kind"], "Overflow");
        assert!(v["detail"].is_null());
    }
}
