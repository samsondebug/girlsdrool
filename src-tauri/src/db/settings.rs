//! Scalar settings (ADR-0030): `setting(key, value_json)`. Every update is a command group with
//! an audit row.

use std::collections::HashMap;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::dates::{now_rfc3339, parse_zone};
use crate::db::audit::{self, Action, Actor};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    pub zone: String,
    pub timing_buffer_cents: i64,
    pub recon_stale_after_days: i64,
    pub dedup_similarity_bps: i64,
    pub theme: String,
    pub backup_keep_daily: i64,
}

pub const KEYS: [&str; 6] = [
    "zone",
    "timing_buffer_cents",
    "recon_stale_after_days",
    "dedup_similarity_bps",
    "theme",
    "backup_keep_daily",
];

pub fn load(conn: &Connection) -> AppResult<Settings> {
    let mut stmt = conn.prepare("SELECT key, value_json FROM setting")?;
    let raw: HashMap<String, serde_json::Value> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .map(|row| {
            let (k, v) = row?;
            let value: serde_json::Value = serde_json::from_str(&v)
                .map_err(|e| AppError::Internal(format!("setting {k} holds invalid JSON: {e}")))?;
            Ok::<_, AppError>((k, value))
        })
        .collect::<Result<_, _>>()?;
    let text = |key: &str| -> AppResult<String> {
        raw.get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| AppError::Internal(format!("setting {key} missing or not text")))
    };
    let int = |key: &str| -> AppResult<i64> {
        raw.get(key)
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| AppError::Internal(format!("setting {key} missing or not an integer")))
    };
    Ok(Settings {
        zone: text("zone")?,
        timing_buffer_cents: int("timing_buffer_cents")?,
        recon_stale_after_days: int("recon_stale_after_days")?,
        dedup_similarity_bps: int("dedup_similarity_bps")?,
        theme: text("theme")?,
        backup_keep_daily: int("backup_keep_daily")?,
    })
}

/// Validate and store one setting as a command group; returns the full settings afterwards.
pub fn update(conn: &mut Connection, key: &str, value: &serde_json::Value) -> AppResult<Settings> {
    validate(key, value)?;
    let tx = conn.transaction()?;
    let (rowid, before): (i64, String) = tx.query_row(
        "SELECT rowid, value_json FROM setting WHERE key = ?1",
        [key],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let before_json: serde_json::Value = serde_json::from_str(&before)?;
    if before_json != *value {
        tx.execute(
            "UPDATE setting SET value_json = ?1, updated_at = ?2 WHERE key = ?3",
            params![value.to_string(), now_rfc3339(), key],
        )?;
        let cmd = audit::begin(&tx, "settings.update", Actor::User)?;
        audit::record(
            &tx,
            &cmd,
            "setting",
            rowid,
            Action::Update,
            Some(&serde_json::json!({ "key": key, "value": before_json })),
            Some(&serde_json::json!({ "key": key, "value": value })),
        )?;
    }
    tx.commit()?;
    load(conn)
}

fn validate(key: &str, value: &serde_json::Value) -> AppResult<()> {
    let need_int = |lo: i64, hi: i64| -> AppResult<()> {
        match value.as_i64() {
            Some(n) if (lo..=hi).contains(&n) => Ok(()),
            _ => Err(AppError::validation(
                key,
                format!("must be an integer between {lo} and {hi}"),
            )),
        }
    };
    match key {
        "zone" => {
            let z = value
                .as_str()
                .ok_or_else(|| AppError::validation(key, "must be an IANA zone name"))?;
            parse_zone(z).map(|_| ())
        }
        "timing_buffer_cents" => need_int(0, i64::MAX),
        "recon_stale_after_days" => need_int(1, 3650),
        "dedup_similarity_bps" => need_int(0, 10_000),
        "theme" => match value.as_str() {
            Some("dark" | "light") => Ok(()),
            _ => Err(AppError::validation(key, "must be \"dark\" or \"light\"")),
        },
        "backup_keep_daily" => need_int(1, 365),
        other => Err(AppError::validation(
            "key",
            format!("{other:?} is not a setting"),
        )),
    }
}
