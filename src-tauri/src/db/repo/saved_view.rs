//! Saved ledger views: a name and the chip text (ADR-0029).

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::dates::now_rfc3339;
use crate::db::audit::{self, Action, CommandRecord};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedView {
    pub id: i64,
    pub name: String,
    pub query_text: String,
    pub created_at: String,
}

fn from_row(r: &rusqlite::Row) -> rusqlite::Result<SavedView> {
    Ok(SavedView {
        id: r.get(0)?,
        name: r.get(1)?,
        query_text: r.get(2)?,
        created_at: r.get(3)?,
    })
}

pub fn list(conn: &Connection) -> AppResult<Vec<SavedView>> {
    let mut stmt =
        conn.prepare("SELECT id, name, query_text, created_at FROM saved_view ORDER BY name")?;
    let rows = stmt
        .query_map([], from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: i64) -> AppResult<SavedView> {
    conn.query_row(
        "SELECT id, name, query_text, created_at FROM saved_view WHERE id = ?1",
        [id],
        from_row,
    )
    .optional()?
    .ok_or(AppError::NotFound {
        entity: "saved_view",
        id,
    })
}

/// Create or replace by name.
pub fn save(
    conn: &Connection,
    cmd: &CommandRecord,
    name: &str,
    query_text: &str,
) -> AppResult<SavedView> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::validation("name", "a saved view needs a name"));
    }
    let existing: Option<SavedView> = conn
        .query_row(
            "SELECT id, name, query_text, created_at FROM saved_view WHERE name = ?1",
            [name],
            from_row,
        )
        .optional()?;
    match existing {
        Some(before) => {
            conn.execute(
                "UPDATE saved_view SET query_text = ?1 WHERE id = ?2",
                params![query_text.trim(), before.id],
            )?;
            let after = get(conn, before.id)?;
            audit::record(
                conn,
                cmd,
                "saved_view",
                before.id,
                Action::Update,
                Some(&serde_json::to_value(&before)?),
                Some(&serde_json::to_value(&after)?),
            )?;
            Ok(after)
        }
        None => {
            conn.execute(
                "INSERT INTO saved_view (name, query_text, created_at) VALUES (?1, ?2, ?3)",
                params![name, query_text.trim(), now_rfc3339()],
            )?;
            let created = get(conn, conn.last_insert_rowid())?;
            audit::record(
                conn,
                cmd,
                "saved_view",
                created.id,
                Action::Insert,
                None,
                Some(&serde_json::to_value(&created)?),
            )?;
            Ok(created)
        }
    }
}

pub fn delete(conn: &Connection, cmd: &CommandRecord, id: i64) -> AppResult<()> {
    let before = get(conn, id)?;
    audit::record(
        conn,
        cmd,
        "saved_view",
        id,
        Action::Delete,
        Some(&serde_json::to_value(&before)?),
        None,
    )?;
    conn.execute("DELETE FROM saved_view WHERE id = ?1", [id])?;
    Ok(())
}
