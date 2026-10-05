//! Categories: a tree with one root per `root_kind`. System rows are editable, never deleted.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::dates::now_rfc3339;
use crate::db::audit::{self, Action, CommandRecord};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Category {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub name: String,
    pub root_kind: String,
    pub is_system: bool,
    pub system_code: Option<String>,
    pub archived: bool,
    pub sort_order: i64,
    /// `Root › Child` for display and chip matching.
    pub path: String,
}

const COLS: &str = "c.id, c.parent_id, c.name, c.root_kind, c.is_system, c.system_code, c.archived, c.sort_order, COALESCE(p.name || ' › ', '') || c.name";

fn from_row(r: &rusqlite::Row) -> rusqlite::Result<Category> {
    Ok(Category {
        id: r.get(0)?,
        parent_id: r.get(1)?,
        name: r.get(2)?,
        root_kind: r.get(3)?,
        is_system: r.get::<_, i64>(4)? == 1,
        system_code: r.get(5)?,
        archived: r.get::<_, i64>(6)? == 1,
        sort_order: r.get(7)?,
        path: r.get(8)?,
    })
}

pub fn list(conn: &Connection) -> AppResult<Vec<Category>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM category c LEFT JOIN category p ON p.id = c.parent_id
         ORDER BY COALESCE(p.sort_order, c.sort_order), c.parent_id IS NOT NULL, c.sort_order, c.name"
    ))?;
    let rows = stmt
        .query_map([], from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Category> {
    conn.query_row(
        &format!("SELECT {COLS} FROM category c LEFT JOIN category p ON p.id = c.parent_id WHERE c.id = ?1"),
        [id],
        from_row,
    )
    .optional()?
    .ok_or(AppError::NotFound {
        entity: "category",
        id,
    })
}

pub fn by_code(conn: &Connection, code: &str) -> AppResult<Category> {
    conn.query_row(
        &format!("SELECT {COLS} FROM category c LEFT JOIN category p ON p.id = c.parent_id WHERE c.system_code = ?1"),
        [code],
        from_row,
    )
    .optional()?
    .ok_or_else(|| AppError::Internal(format!("category seed {code} is missing")))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewCategory {
    pub parent_id: i64,
    pub name: String,
}

/// Create a child under an existing category (roots are seeded, never created).
pub fn create(conn: &Connection, cmd: &CommandRecord, new: &NewCategory) -> AppResult<Category> {
    let name = new.name.trim();
    if name.is_empty() {
        return Err(AppError::validation("name", "a category needs a name"));
    }
    let parent = get(conn, new.parent_id)?;
    conn.execute(
        "INSERT INTO category (parent_id, name, root_kind, is_system, sort_order, created_at)
         VALUES (?1, ?2, ?3, 0, (SELECT COALESCE(MAX(sort_order), 0) + 10 FROM category WHERE parent_id = ?1), ?4)",
        params![parent.id, name, parent.root_kind, now_rfc3339()],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation => {
            AppError::validation("name", format!("{:?} already exists under {}", name, parent.name))
        }
        other => other.into(),
    })?;
    let created = get(conn, conn.last_insert_rowid())?;
    audit::record(
        conn,
        cmd,
        "category",
        created.id,
        Action::Insert,
        None,
        Some(&serde_json::to_value(&created)?),
    )?;
    Ok(created)
}

pub fn rename(conn: &Connection, cmd: &CommandRecord, id: i64, name: &str) -> AppResult<Category> {
    let before = get(conn, id)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::validation("name", "a category needs a name"));
    }
    conn.execute(
        "UPDATE category SET name = ?1 WHERE id = ?2",
        params![name, id],
    )?;
    let after = get(conn, id)?;
    audit::record(
        conn,
        cmd,
        "category",
        id,
        Action::Update,
        Some(&serde_json::to_value(&before)?),
        Some(&serde_json::to_value(&after)?),
    )?;
    Ok(after)
}

/// Archive a user category (system rows cannot be archived or deleted).
pub fn archive(
    conn: &Connection,
    cmd: &CommandRecord,
    id: i64,
    archived: bool,
) -> AppResult<Category> {
    let before = get(conn, id)?;
    if before.is_system {
        return Err(AppError::Conflict(
            "system categories cannot be archived".into(),
        ));
    }
    conn.execute(
        "UPDATE category SET archived = ?1 WHERE id = ?2",
        params![i64::from(archived), id],
    )?;
    let after = get(conn, id)?;
    audit::record(
        conn,
        cmd,
        "category",
        id,
        Action::Update,
        Some(&serde_json::to_value(&before)?),
        Some(&serde_json::to_value(&after)?),
    )?;
    Ok(after)
}
