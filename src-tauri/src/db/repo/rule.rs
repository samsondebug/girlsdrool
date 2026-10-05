//! Ordered categorisation rules (ARCHITECTURE §6.4). Rules are user data: created, edited,
//! reordered and deleted here, every change audited. Nothing creates a rule but the user.

use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::dates::now_rfc3339;
use crate::db::audit::{self, Action, CommandRecord};
use crate::db::repo::txn::flags_from_names;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rule {
    pub id: i64,
    pub position: i64,
    pub name: String,
    pub enabled: bool,
    pub match_payee_contains: Option<String>,
    pub match_payee_regex: Option<String>,
    pub match_memo_contains: Option<String>,
    pub match_amount_min_cents: Option<i64>,
    pub match_amount_max_cents: Option<i64>,
    pub match_account_id: Option<i64>,
    pub action_category_id: Option<i64>,
    pub action_venture_id: Option<i64>,
    pub action_flags_set: i64,
    pub action_tag_ids: Vec<i64>,
    pub hit_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// What the user fills in. Every field is optional except the name; at least one match and one
/// action are required.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RuleInput {
    pub name: String,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub match_payee_contains: Option<String>,
    #[serde(default)]
    pub match_payee_regex: Option<String>,
    #[serde(default)]
    pub match_memo_contains: Option<String>,
    #[serde(default)]
    pub match_amount_min_cents: Option<i64>,
    #[serde(default)]
    pub match_amount_max_cents: Option<i64>,
    #[serde(default)]
    pub match_account_id: Option<i64>,
    #[serde(default)]
    pub action_category_id: Option<i64>,
    #[serde(default)]
    pub action_venture_id: Option<i64>,
    #[serde(default)]
    pub action_flags: Vec<String>,
    #[serde(default)]
    pub action_tag_ids: Vec<i64>,
}

const COLS: &str = "id, position, name, enabled, match_payee_contains, match_payee_regex, match_memo_contains, match_amount_min_cents, match_amount_max_cents, match_account_id, action_category_id, action_venture_id, action_flags_set, action_tag_ids_json, hit_count, created_at, updated_at";

fn from_row(r: &rusqlite::Row) -> rusqlite::Result<Rule> {
    let tags_json: String = r.get(13)?;
    Ok(Rule {
        id: r.get(0)?,
        position: r.get(1)?,
        name: r.get(2)?,
        enabled: r.get::<_, i64>(3)? == 1,
        match_payee_contains: r.get(4)?,
        match_payee_regex: r.get(5)?,
        match_memo_contains: r.get(6)?,
        match_amount_min_cents: r.get(7)?,
        match_amount_max_cents: r.get(8)?,
        match_account_id: r.get(9)?,
        action_category_id: r.get(10)?,
        action_venture_id: r.get(11)?,
        action_flags_set: r.get(12)?,
        action_tag_ids: serde_json::from_str(&tags_json).unwrap_or_default(),
        hit_count: r.get(14)?,
        created_at: r.get(15)?,
        updated_at: r.get(16)?,
    })
}

pub fn list(conn: &Connection) -> AppResult<Vec<Rule>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM rule ORDER BY position, id"))?;
    let rows = stmt
        .query_map([], from_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Rule> {
    conn.query_row(
        &format!("SELECT {COLS} FROM rule WHERE id = ?1"),
        [id],
        from_row,
    )
    .optional()?
    .ok_or(AppError::NotFound { entity: "rule", id })
}

fn clean(s: &Option<String>) -> Option<String> {
    s.as_ref()
        .map(|v| v.trim().to_lowercase())
        .filter(|v| !v.is_empty())
}

fn validate(conn: &Connection, input: &RuleInput) -> AppResult<()> {
    if input.name.trim().is_empty() {
        return Err(AppError::validation("name", "a rule needs a name"));
    }
    let has_match = clean(&input.match_payee_contains).is_some()
        || clean(&input.match_payee_regex).is_some()
        || clean(&input.match_memo_contains).is_some()
        || input.match_amount_min_cents.is_some()
        || input.match_amount_max_cents.is_some()
        || input.match_account_id.is_some();
    if !has_match {
        return Err(AppError::validation(
            "match",
            "a rule needs at least one match condition",
        ));
    }
    if let Some(re) = clean(&input.match_payee_regex) {
        Regex::new(&re).map_err(|e| {
            AppError::validation("match_payee_regex", format!("invalid regex: {e}"))
        })?;
    }
    if let (Some(lo), Some(hi)) = (input.match_amount_min_cents, input.match_amount_max_cents) {
        if lo > hi {
            return Err(AppError::validation(
                "match_amount",
                "the amount band's minimum is above its maximum",
            ));
        }
    }
    if input.action_category_id.is_none()
        && input.action_venture_id.is_none()
        && input.action_flags.is_empty()
        && input.action_tag_ids.is_empty()
    {
        return Err(AppError::validation(
            "action",
            "a rule needs at least one action",
        ));
    }
    if let Some(cid) = input.action_category_id {
        let n: i64 = conn.query_row("SELECT count(*) FROM category WHERE id = ?1", [cid], |r| {
            r.get(0)
        })?;
        if n == 0 {
            return Err(AppError::NotFound {
                entity: "category",
                id: cid,
            });
        }
    }
    if let Some(vid) = input.action_venture_id {
        let n: i64 = conn.query_row("SELECT count(*) FROM venture WHERE id = ?1", [vid], |r| {
            r.get(0)
        })?;
        if n == 0 {
            return Err(AppError::NotFound {
                entity: "venture",
                id: vid,
            });
        }
    }
    if let Some(aid) = input.match_account_id {
        let n: i64 = conn.query_row("SELECT count(*) FROM account WHERE id = ?1", [aid], |r| {
            r.get(0)
        })?;
        if n == 0 {
            return Err(AppError::NotFound {
                entity: "account",
                id: aid,
            });
        }
    }
    for tid in &input.action_tag_ids {
        let n: i64 = conn.query_row("SELECT count(*) FROM tag WHERE id = ?1", [tid], |r| {
            r.get(0)
        })?;
        if n == 0 {
            return Err(AppError::NotFound {
                entity: "tag",
                id: *tid,
            });
        }
    }
    Ok(())
}

/// Append a rule at the end of the order.
pub fn create(conn: &Connection, cmd: &CommandRecord, input: &RuleInput) -> AppResult<Rule> {
    validate(conn, input)?;
    let flags = flags_from_names(&input.action_flags)?;
    let now = now_rfc3339();
    conn.execute(
        "INSERT INTO rule (position, name, enabled, match_payee_contains, match_payee_regex, match_memo_contains, match_amount_min_cents,
                           match_amount_max_cents, match_account_id, action_category_id, action_venture_id, action_flags_set, action_tag_ids_json,
                           hit_count, created_at, updated_at)
         VALUES ((SELECT COALESCE(MAX(position), 0) + 10 FROM rule), ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 0, ?13, ?13)",
        params![
            input.name.trim(),
            i64::from(input.enabled.unwrap_or(true)),
            clean(&input.match_payee_contains),
            clean(&input.match_payee_regex),
            clean(&input.match_memo_contains),
            input.match_amount_min_cents,
            input.match_amount_max_cents,
            input.match_account_id,
            input.action_category_id,
            input.action_venture_id,
            flags,
            serde_json::to_string(&input.action_tag_ids)?,
            now
        ],
    )?;
    let rule = get(conn, conn.last_insert_rowid())?;
    audit::record(
        conn,
        cmd,
        "rule",
        rule.id,
        Action::Insert,
        None,
        Some(&serde_json::to_value(&rule)?),
    )?;
    Ok(rule)
}

pub fn update(
    conn: &Connection,
    cmd: &CommandRecord,
    id: i64,
    input: &RuleInput,
) -> AppResult<Rule> {
    let before = get(conn, id)?;
    validate(conn, input)?;
    let flags = flags_from_names(&input.action_flags)?;
    conn.execute(
        "UPDATE rule SET name = ?1, enabled = ?2, match_payee_contains = ?3, match_payee_regex = ?4, match_memo_contains = ?5,
         match_amount_min_cents = ?6, match_amount_max_cents = ?7, match_account_id = ?8, action_category_id = ?9, action_venture_id = ?10,
         action_flags_set = ?11, action_tag_ids_json = ?12, updated_at = ?13 WHERE id = ?14",
        params![
            input.name.trim(),
            i64::from(input.enabled.unwrap_or(before.enabled)),
            clean(&input.match_payee_contains),
            clean(&input.match_payee_regex),
            clean(&input.match_memo_contains),
            input.match_amount_min_cents,
            input.match_amount_max_cents,
            input.match_account_id,
            input.action_category_id,
            input.action_venture_id,
            flags,
            serde_json::to_string(&input.action_tag_ids)?,
            now_rfc3339(),
            id
        ],
    )?;
    let after = get(conn, id)?;
    audit::record(
        conn,
        cmd,
        "rule",
        id,
        Action::Update,
        Some(&serde_json::to_value(&before)?),
        Some(&serde_json::to_value(&after)?),
    )?;
    Ok(after)
}

/// Delete a rule. Rows it categorised keep their category but lose the `rule_id` pointer, so the
/// ledger says "rule (deleted)" rather than pointing at nothing.
pub fn delete(conn: &Connection, cmd: &CommandRecord, id: i64) -> AppResult<()> {
    let before = get(conn, id)?;
    conn.execute("UPDATE txn SET rule_id = NULL WHERE rule_id = ?1", [id])?;
    audit::record(
        conn,
        cmd,
        "rule",
        id,
        Action::Delete,
        Some(&serde_json::to_value(&before)?),
        None,
    )?;
    conn.execute("DELETE FROM rule WHERE id = ?1", [id])?;
    Ok(())
}

/// Set the evaluation order to exactly these ids (every existing rule must appear once).
pub fn reorder(conn: &Connection, cmd: &CommandRecord, ids: &[i64]) -> AppResult<Vec<Rule>> {
    let existing = list(conn)?;
    let mut sorted_existing: Vec<i64> = existing.iter().map(|r| r.id).collect();
    sorted_existing.sort_unstable();
    let mut sorted_ids = ids.to_vec();
    sorted_ids.sort_unstable();
    if sorted_existing != sorted_ids {
        return Err(AppError::validation(
            "ids",
            "the new order must list every rule exactly once",
        ));
    }
    for (i, id) in ids.iter().enumerate() {
        let before = existing.iter().find(|r| r.id == *id).cloned();
        let position = i64::try_from((i + 1) * 10).map_err(|_| AppError::Overflow)?;
        conn.execute(
            "UPDATE rule SET position = ?1 WHERE id = ?2",
            params![position, id],
        )?;
        if let Some(b) = before {
            if b.position != position {
                let after = get(conn, *id)?;
                audit::record(
                    conn,
                    cmd,
                    "rule",
                    *id,
                    Action::Update,
                    Some(&serde_json::to_value(&b)?),
                    Some(&serde_json::to_value(&after)?),
                )?;
            }
        }
    }
    list(conn)
}

pub fn bump_hits(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute(
        "UPDATE rule SET hit_count = hit_count + 1 WHERE id = ?1",
        [id],
    )?;
    Ok(())
}
