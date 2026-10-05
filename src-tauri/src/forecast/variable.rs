//! The variable-spend model (ARCHITECTURE §5.7): per variable category, three trailing 30-day
//! buckets ending yesterday, the median of the three, and the user's override when one exists.

use chrono::Duration;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::dates::{format_civil, CivilDate};
use crate::db::audit::{self, Action, CommandRecord};
use crate::db::repo::category;
use crate::error::{AppError, AppResult};

/// Length of one bucket and of one forecast block the model is spread over.
pub const BUCKET_DAYS: i64 = 30;
/// Number of trailing buckets the median is taken over.
pub const BUCKETS: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Bucket {
    pub start: String,
    pub end: String,
    /// `max(0, −Σ amount)` of posted, non-transfer leaf rows in the category: refunds net.
    pub net_outflow_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CategoryModel {
    pub category_id: i64,
    pub code: Option<String>,
    pub name: String,
    pub buckets: Vec<Bucket>,
    pub median_cents: i64,
    pub override_cents: Option<i64>,
    /// What the forecast spends: the override when set, else the median.
    pub per_30_days_cents: i64,
}

/// Every child of the variable root, in seed order, with its buckets, median and override.
pub fn model(conn: &Connection, today: CivilDate) -> AppResult<Vec<CategoryModel>> {
    let mut out = Vec::new();
    for cat in category::list(conn)?
        .into_iter()
        .filter(|c| c.root_kind == "variable" && c.parent_id.is_some() && !c.archived)
    {
        let mut buckets = Vec::with_capacity(BUCKETS);
        for k in 0..BUCKETS {
            let end = today - Duration::days(1 + BUCKET_DAYS * k as i64);
            let start = end - Duration::days(BUCKET_DAYS - 1);
            let sum: i64 = conn.query_row(
                "SELECT COALESCE(SUM(amount_cents), 0) FROM txn_leaf
                 WHERE category_id = ?1 AND status = 'posted' AND transfer_link_id IS NULL
                   AND posted_date >= ?2 AND posted_date <= ?3",
                params![cat.id, format_civil(start), format_civil(end)],
                |r| r.get(0),
            )?;
            buckets.push(Bucket {
                start: format_civil(start),
                end: format_civil(end),
                net_outflow_cents: sum.checked_neg().ok_or(AppError::Overflow)?.max(0),
            });
        }
        let mut sorted: Vec<i64> = buckets.iter().map(|b| b.net_outflow_cents).collect();
        sorted.sort_unstable();
        let median_cents = sorted[BUCKETS / 2];
        let override_cents = override_for(conn, cat.id)?;
        out.push(CategoryModel {
            category_id: cat.id,
            code: cat.system_code.clone(),
            name: cat.name.clone(),
            buckets,
            median_cents,
            override_cents,
            per_30_days_cents: override_cents.unwrap_or(median_cents),
        });
    }
    Ok(out)
}

pub fn override_for(conn: &Connection, category_id: i64) -> AppResult<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT per_30_days_cents FROM variable_spend_override WHERE category_id = ?1",
            [category_id],
            |r| r.get(0),
        )
        .optional()?)
}

/// Set (or with `None` clear) the per-30-days figure the forecast uses instead of the median.
pub fn set_override(
    conn: &Connection,
    cmd: &CommandRecord,
    category_id: i64,
    per_30_days_cents: Option<i64>,
) -> AppResult<()> {
    let cat = category::get(conn, category_id)?;
    if cat.root_kind != "variable" || cat.parent_id.is_none() {
        return Err(AppError::validation(
            "category_id",
            "only a variable category has a spend model",
        ));
    }
    let before = override_for(conn, category_id)?;
    let before_json =
        before.map(|c| serde_json::json!({ "category_id": category_id, "per_30_days_cents": c }));
    match per_30_days_cents {
        Some(cents) => {
            if cents < 0 {
                return Err(AppError::validation(
                    "per_30_days_cents",
                    "an override cannot be negative",
                ));
            }
            if before == Some(cents) {
                return Ok(());
            }
            conn.execute(
                "INSERT INTO variable_spend_override (category_id, per_30_days_cents, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(category_id) DO UPDATE SET per_30_days_cents = excluded.per_30_days_cents, updated_at = excluded.updated_at",
                params![category_id, cents, crate::dates::now_rfc3339()],
            )?;
            let after =
                serde_json::json!({ "category_id": category_id, "per_30_days_cents": cents });
            audit::record(
                conn,
                cmd,
                "variable_spend_override",
                category_id,
                if before.is_some() {
                    Action::Update
                } else {
                    Action::Insert
                },
                before_json.as_ref(),
                Some(&after),
            )
        }
        None => {
            if before.is_none() {
                return Ok(());
            }
            conn.execute(
                "DELETE FROM variable_spend_override WHERE category_id = ?1",
                [category_id],
            )?;
            audit::record(
                conn,
                cmd,
                "variable_spend_override",
                category_id,
                Action::Delete,
                before_json.as_ref(),
                None,
            )
        }
    }
}
