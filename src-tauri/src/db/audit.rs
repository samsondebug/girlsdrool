//! Command groups and audit rows (ADR-0016). Every write command opens one `command` row and
//! records one `audit_event` per touched row, inside the same transaction as the write.

use rusqlite::{params, Connection};

use crate::dates::now_rfc3339;
use crate::error::AppResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Actor {
    User,
    Import,
    System,
    Undo,
}

impl Actor {
    pub fn as_str(self) -> &'static str {
        match self {
            Actor::User => "user",
            Actor::Import => "import",
            Actor::System => "system",
            Actor::Undo => "undo",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Insert,
    Update,
    Delete,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Action::Insert => "insert",
            Action::Update => "update",
            Action::Delete => "delete",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandRecord {
    pub id: i64,
}

/// Start a command group. Call inside the transaction that performs the writes.
pub fn begin(conn: &Connection, name: &str, actor: Actor) -> AppResult<CommandRecord> {
    conn.execute(
        "INSERT INTO command (name, actor, at) VALUES (?1, ?2, ?3)",
        params![name, actor.as_str(), now_rfc3339()],
    )?;
    Ok(CommandRecord {
        id: conn.last_insert_rowid(),
    })
}

/// Record one row change. `before` is `None` on insert, `after` is `None` on delete.
pub fn record(
    conn: &Connection,
    cmd: &CommandRecord,
    entity: &str,
    entity_id: i64,
    action: Action,
    before: Option<&serde_json::Value>,
    after: Option<&serde_json::Value>,
) -> AppResult<()> {
    conn.prepare_cached(
        "INSERT INTO audit_event (command_id, at, entity, entity_id, action, before_json, after_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?
    .execute(params![
            cmd.id,
            now_rfc3339(),
            entity,
            entity_id,
            action.as_str(),
            before.map(serde_json::Value::to_string),
            after.map(serde_json::Value::to_string),
        ],
    )?;
    Ok(())
}
