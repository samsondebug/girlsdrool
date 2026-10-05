//! Policies: declarative rules. The two system policies are enforced by the engines
//! (`firewall_exclusion` in safe-to-spend and the review queue, `informal_first` in debt
//! strategies, M6); user policies the engine does not understand are listed as reminders.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Policy {
    pub id: i64,
    pub code: Option<String>,
    pub name: String,
    pub kind: String,
    pub params_json: String,
    pub is_system: bool,
    pub created_at: String,
}

pub fn list(conn: &Connection) -> AppResult<Vec<Policy>> {
    let mut stmt = conn.prepare(
        "SELECT id, code, name, kind, params_json, is_system, created_at FROM policy ORDER BY is_system DESC, id",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Policy {
                id: r.get(0)?,
                code: r.get(1)?,
                name: r.get(2)?,
                kind: r.get(3)?,
                params_json: r.get(4)?,
                is_system: r.get::<_, i64>(5)? != 0,
                created_at: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
