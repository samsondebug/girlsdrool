//! Accounts. Never deleted: archived instead (the ledger is append-only in spirit).

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::dates::{now_rfc3339, parse_civil};
use crate::db::audit::{self, Action, CommandRecord};
use crate::error::{AppError, AppResult};

pub const KINDS: [&str; 8] = [
    "checking",
    "savings",
    "credit",
    "brokerage",
    "loan",
    "payment_app",
    "cash",
    "venture",
];

/// Kinds whose balance is cash from the owner's point of view (ARCHITECTURE §5.4, set A).
pub const CASH_KINDS: [&str; 4] = ["checking", "savings", "cash", "payment_app"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    pub id: i64,
    pub name: String,
    pub institution: String,
    pub kind: String,
    pub currency: String,
    pub opening_balance_cents: i64,
    pub opening_date: String,
    pub owner: String,
    pub venture_id: Option<i64>,
    pub firewalled: bool,
    pub archived: bool,
    pub recon_stale_after_days: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewAccount {
    pub name: String,
    #[serde(default)]
    pub institution: String,
    pub kind: String,
    pub opening_balance_cents: i64,
    pub opening_date: String,
    #[serde(default)]
    pub venture_id: Option<i64>,
    #[serde(default)]
    pub firewalled: bool,
}

/// Fields a user may change after creation. Kind and opening figures change only while the
/// account has no rows (otherwise every balance would shift under the ledger).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AccountPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub institution: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub opening_balance_cents: Option<i64>,
    #[serde(default)]
    pub opening_date: Option<String>,
    #[serde(default)]
    pub firewalled: Option<bool>,
    #[serde(default)]
    pub archived: Option<bool>,
    #[serde(default, deserialize_with = "crate::db::repo::double_option")]
    pub recon_stale_after_days: Option<Option<i64>>,
}

const COLS: &str = "id, name, institution, kind, currency, opening_balance_cents, opening_date, owner, venture_id, firewalled, archived, recon_stale_after_days, created_at";

fn from_row(r: &rusqlite::Row) -> rusqlite::Result<Account> {
    Ok(Account {
        id: r.get(0)?,
        name: r.get(1)?,
        institution: r.get(2)?,
        kind: r.get(3)?,
        currency: r.get(4)?,
        opening_balance_cents: r.get(5)?,
        opening_date: r.get(6)?,
        owner: r.get(7)?,
        venture_id: r.get(8)?,
        firewalled: r.get::<_, i64>(9)? == 1,
        archived: r.get::<_, i64>(10)? == 1,
        recon_stale_after_days: r.get(11)?,
        created_at: r.get(12)?,
    })
}

pub fn list(conn: &Connection) -> AppResult<Vec<Account>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM account ORDER BY archived, name"
    ))?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Account> {
    conn.query_row(
        &format!("SELECT {COLS} FROM account WHERE id = ?1"),
        [id],
        from_row,
    )
    .optional()?
    .ok_or(AppError::NotFound {
        entity: "account",
        id,
    })
}

fn validate_kind(kind: &str) -> AppResult<()> {
    if KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(AppError::validation(
            "kind",
            format!("{kind:?} is not an account kind"),
        ))
    }
}

pub fn create(conn: &Connection, cmd: &CommandRecord, new: &NewAccount) -> AppResult<Account> {
    let name = new.name.trim();
    if name.is_empty() {
        return Err(AppError::validation("name", "an account needs a name"));
    }
    validate_kind(&new.kind)?;
    parse_civil(&new.opening_date)?;
    let owner = if new.venture_id.is_some() {
        "venture"
    } else {
        "personal"
    };
    if new.kind == "venture" && new.venture_id.is_none() {
        return Err(AppError::validation(
            "venture_id",
            "a venture account belongs to a venture",
        ));
    }
    conn.execute(
        "INSERT INTO account (name, institution, kind, currency, opening_balance_cents, opening_date, owner, venture_id, firewalled, archived, created_at)
         VALUES (?1, ?2, ?3, 'USD', ?4, ?5, ?6, ?7, ?8, 0, ?9)",
        params![
            name,
            new.institution.trim(),
            new.kind,
            new.opening_balance_cents,
            new.opening_date,
            owner,
            new.venture_id,
            i64::from(new.firewalled),
            now_rfc3339()
        ],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation => {
            AppError::validation("name", format!("an account named {name:?} already exists"))
        }
        other => other.into(),
    })?;
    let account = get(conn, conn.last_insert_rowid())?;
    audit::record(
        conn,
        cmd,
        "account",
        account.id,
        Action::Insert,
        None,
        Some(&serde_json::to_value(&account)?),
    )?;
    Ok(account)
}

pub fn row_count(conn: &Connection, account_id: i64) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT count(*) FROM txn WHERE account_id = ?1",
        [account_id],
        |r| r.get(0),
    )?)
}

pub fn update(
    conn: &Connection,
    cmd: &CommandRecord,
    id: i64,
    patch: &AccountPatch,
) -> AppResult<Account> {
    let before = get(conn, id)?;
    let mut after = before.clone();
    if let Some(name) = &patch.name {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::validation("name", "an account needs a name"));
        }
        after.name = name.to_string();
    }
    if let Some(institution) = &patch.institution {
        after.institution = institution.trim().to_string();
    }
    let has_rows = row_count(conn, id)? > 0;
    if let Some(kind) = &patch.kind {
        validate_kind(kind)?;
        if has_rows && *kind != before.kind {
            return Err(AppError::Conflict(
                "the kind of an account with rows cannot change".into(),
            ));
        }
        after.kind = kind.clone();
    }
    if let Some(cents) = patch.opening_balance_cents {
        if has_rows && cents != before.opening_balance_cents {
            return Err(AppError::Conflict(
                "the opening balance of an account with rows cannot change; reconcile instead"
                    .into(),
            ));
        }
        after.opening_balance_cents = cents;
    }
    if let Some(date) = &patch.opening_date {
        parse_civil(date)?;
        if has_rows && *date != before.opening_date {
            return Err(AppError::Conflict(
                "the opening date of an account with rows cannot change".into(),
            ));
        }
        after.opening_date = date.clone();
    }
    if let Some(firewalled) = patch.firewalled {
        after.firewalled = firewalled;
    }
    if let Some(archived) = patch.archived {
        after.archived = archived;
    }
    if let Some(days) = patch.recon_stale_after_days {
        if let Some(d) = days {
            if !(1..=3650).contains(&d) {
                return Err(AppError::validation(
                    "recon_stale_after_days",
                    "must be between 1 and 3650",
                ));
            }
        }
        after.recon_stale_after_days = days;
    }
    if after == before {
        return Ok(before);
    }
    conn.execute(
        "UPDATE account SET name = ?1, institution = ?2, kind = ?3, opening_balance_cents = ?4, opening_date = ?5,
         firewalled = ?6, archived = ?7, recon_stale_after_days = ?8 WHERE id = ?9",
        params![
            after.name,
            after.institution,
            after.kind,
            after.opening_balance_cents,
            after.opening_date,
            i64::from(after.firewalled),
            i64::from(after.archived),
            after.recon_stale_after_days,
            id
        ],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation => {
            AppError::validation("name", format!("an account named {:?} already exists", after.name))
        }
        other => other.into(),
    })?;
    audit::record(
        conn,
        cmd,
        "account",
        id,
        Action::Update,
        Some(&serde_json::to_value(&before)?),
        Some(&serde_json::to_value(&after)?),
    )?;
    Ok(after)
}
