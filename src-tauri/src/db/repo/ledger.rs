//! The ledger query (ARCHITECTURE §15, §16): chip filters compiled to indexed SQL over
//! `txn_leaf`, keyset-paginated on `(posted_date, id)`, with the filtered total computed here —
//! the webview never sums.

use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection};
use serde::{Deserialize, Serialize};

use crate::db::repo::{placeholders, push_ids};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct LedgerFilter {
    #[serde(default)]
    pub account_ids: Vec<i64>,
    #[serde(default)]
    pub category_ids: Vec<i64>,
    #[serde(default)]
    pub tag_names: Vec<String>,
    #[serde(default)]
    pub venture_ids: Vec<i64>,
    /// `>100`: |amount| strictly greater than this many cents.
    #[serde(default)]
    pub abs_gt_cents: Option<i64>,
    #[serde(default)]
    pub abs_ge_cents: Option<i64>,
    #[serde(default)]
    pub abs_lt_cents: Option<i64>,
    #[serde(default)]
    pub abs_le_cents: Option<i64>,
    #[serde(default)]
    pub abs_eq_cents: Option<i64>,
    #[serde(default)]
    pub needs_review: bool,
    #[serde(default)]
    pub unclassified: bool,
    #[serde(default)]
    pub flags_any: i64,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub date_from: Option<String>,
    #[serde(default)]
    pub date_to: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Cursor {
    pub posted_date: String,
    pub id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerRow {
    pub id: i64,
    pub account_id: i64,
    pub account_name: String,
    pub parent_id: Option<i64>,
    pub posted_date: String,
    pub effective_date: String,
    pub amount_cents: i64,
    pub payee_raw: String,
    pub payee_norm: String,
    pub memo: String,
    pub category_id: Option<i64>,
    pub category_path: Option<String>,
    pub classification: String,
    pub rule_id: Option<i64>,
    pub heuristic_code: Option<String>,
    pub user_edited: i64,
    pub status: String,
    pub transfer_link_id: Option<i64>,
    pub refund_link_id: Option<i64>,
    pub venture_id: Option<i64>,
    pub flags: i64,
    pub import_batch_id: Option<i64>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerPage {
    pub rows: Vec<LedgerRow>,
    pub next_cursor: Option<Cursor>,
    /// Rows matching the filter (all pages).
    pub total_rows: i64,
    /// Σ amount_cents over every matching leaf row.
    pub total_cents: i64,
}

struct Built {
    where_sql: String,
    params: Vec<Value>,
}

/// Collects positional parameters and hands back their `?N` placeholders.
struct Params {
    values: Vec<Value>,
}

impl Params {
    fn push(&mut self, v: Value) -> String {
        self.values.push(v);
        format!("?{}", self.values.len())
    }

    fn push_ids(&mut self, ids: &[i64]) -> String {
        let start = self.values.len() + 1;
        push_ids(&mut self.values, ids);
        placeholders(start, ids.len())
    }
}

fn build_where(f: &LedgerFilter) -> AppResult<Built> {
    let mut conds: Vec<String> = Vec::new();
    let mut p = Params { values: Vec::new() };

    if !f.account_ids.is_empty() {
        let list = p.push_ids(&f.account_ids);
        conds.push(format!("t.account_id IN ({list})"));
    }
    if !f.category_ids.is_empty() {
        let list = p.push_ids(&f.category_ids);
        conds.push(format!("t.category_id IN ({list})"));
    }
    if !f.venture_ids.is_empty() {
        let list = p.push_ids(&f.venture_ids);
        conds.push(format!("t.venture_id IN ({list})"));
    }
    for name in &f.tag_names {
        let ph = p.push(Value::Text(name.clone()));
        conds.push(format!(
            "EXISTS (SELECT 1 FROM txn_tag x JOIN tag g ON g.id = x.tag_id WHERE x.txn_id = t.id AND g.name = {ph} COLLATE NOCASE)"
        ));
    }
    let abs_conds: [(Option<i64>, &str); 5] = [
        (f.abs_gt_cents, ">"),
        (f.abs_ge_cents, ">="),
        (f.abs_lt_cents, "<"),
        (f.abs_le_cents, "<="),
        (f.abs_eq_cents, "="),
    ];
    for (value, op) in abs_conds {
        if let Some(c) = value {
            let ph = p.push(Value::Integer(c));
            conds.push(format!("abs(t.amount_cents) {op} {ph}"));
        }
    }
    if f.needs_review {
        conds.push("(t.flags & 1) <> 0".into());
    }
    if f.unclassified {
        conds.push("t.classification = 'unclassified'".into());
    }
    if f.flags_any != 0 {
        let ph = p.push(Value::Integer(f.flags_any));
        conds.push(format!("(t.flags & {ph}) <> 0"));
    }
    if let Some(s) = &f.status {
        if s != "pending" && s != "posted" {
            return Err(AppError::validation("status", "must be pending or posted"));
        }
        let ph = p.push(Value::Text(s.clone()));
        conds.push(format!("t.status = {ph}"));
    }
    if let Some(d) = &f.date_from {
        let ph = p.push(Value::Text(d.clone()));
        conds.push(format!("t.posted_date >= {ph}"));
    }
    if let Some(d) = &f.date_to {
        let ph = p.push(Value::Text(d.clone()));
        conds.push(format!("t.posted_date <= {ph}"));
    }
    if let Some(text) = &f.text {
        let needle = format!(
            "%{}%",
            text.trim()
                .to_lowercase()
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        let ph = p.push(Value::Text(needle));
        conds.push(format!(
            "(t.payee_norm LIKE {ph} ESCAPE '\\' OR lower(t.payee_raw) LIKE {ph} ESCAPE '\\' OR lower(t.memo) LIKE {ph} ESCAPE '\\')"
        ));
    }
    let where_sql = if conds.is_empty() {
        "1 = 1".to_string()
    } else {
        conds.join(" AND ")
    };
    Ok(Built {
        where_sql,
        params: p.values,
    })
}

const ROW_SELECT: &str = "SELECT t.id, t.account_id, a.name, t.parent_id, t.posted_date, t.effective_date, t.amount_cents, t.payee_raw,
    t.payee_norm, t.memo, t.category_id, CASE WHEN c.id IS NULL THEN NULL ELSE COALESCE(cp.name || ' › ', '') || c.name END,
    t.classification, t.rule_id, t.heuristic_code, t.user_edited, t.status, t.transfer_link_id, t.refund_link_id, t.venture_id,
    t.flags, t.import_batch_id
  FROM txn_leaf t
  JOIN account a ON a.id = t.account_id
  LEFT JOIN category c ON c.id = t.category_id
  LEFT JOIN category cp ON cp.id = c.parent_id";

fn row_from(r: &rusqlite::Row) -> rusqlite::Result<LedgerRow> {
    Ok(LedgerRow {
        id: r.get(0)?,
        account_id: r.get(1)?,
        account_name: r.get(2)?,
        parent_id: r.get(3)?,
        posted_date: r.get(4)?,
        effective_date: r.get(5)?,
        amount_cents: r.get(6)?,
        payee_raw: r.get(7)?,
        payee_norm: r.get(8)?,
        memo: r.get(9)?,
        category_id: r.get(10)?,
        category_path: r.get(11)?,
        classification: r.get(12)?,
        rule_id: r.get(13)?,
        heuristic_code: r.get(14)?,
        user_edited: r.get(15)?,
        status: r.get(16)?,
        transfer_link_id: r.get(17)?,
        refund_link_id: r.get(18)?,
        venture_id: r.get(19)?,
        flags: r.get(20)?,
        import_batch_id: r.get(21)?,
        tags: Vec::new(),
    })
}

/// One page of leaf rows, newest first, after `cursor` when given.
pub fn query(
    conn: &Connection,
    filter: &LedgerFilter,
    cursor: Option<&Cursor>,
    limit: usize,
) -> AppResult<LedgerPage> {
    let limit = limit.clamp(1, 500);
    let built = build_where(filter)?;

    let (total_rows, total_cents): (i64, i64) = conn.query_row(
        &format!(
            "SELECT count(*), COALESCE(SUM(t.amount_cents), 0) FROM txn_leaf t WHERE {}",
            built.where_sql
        ),
        params_from_iter(built.params.iter()),
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    let mut params = built.params.clone();
    let mut cursor_sql = String::new();
    if let Some(c) = cursor {
        params.push(Value::Text(c.posted_date.clone()));
        params.push(Value::Integer(c.id));
        let n = params.len();
        cursor_sql = format!(
            " AND (t.posted_date < ?{} OR (t.posted_date = ?{} AND t.id < ?{}))",
            n - 1,
            n - 1,
            n
        );
    }
    params.push(Value::Integer(
        i64::try_from(limit + 1).map_err(|_| AppError::Overflow)?,
    ));
    let limit_param = params.len();
    let sql = format!(
        "{ROW_SELECT} WHERE {}{cursor_sql} ORDER BY t.posted_date DESC, t.id DESC LIMIT ?{limit_param}",
        built.where_sql
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt
        .query_map(params_from_iter(params.iter()), row_from)?
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = if rows.len() > limit {
        rows.truncate(limit);
        rows.last().map(|r| Cursor {
            posted_date: r.posted_date.clone(),
            id: r.id,
        })
    } else {
        None
    };
    for row in &mut rows {
        row.tags = crate::db::repo::txn::tags_of(conn, row.id)?;
    }
    Ok(LedgerPage {
        rows,
        next_cursor,
        total_rows,
        total_cents,
    })
}

/// Σ posted leaf amounts per account (opening balance excluded), for balances.
pub fn posted_sum(conn: &Connection, account_id: i64) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(amount_cents), 0) FROM txn_leaf WHERE account_id = ?1 AND status = 'posted'",
        [account_id],
        |r| r.get(0),
    )?)
}

/// The review queue: rows flagged for review or still unclassified, largest first.
pub fn review_queue(conn: &Connection, limit: usize) -> AppResult<Vec<LedgerRow>> {
    let limit = i64::try_from(limit.clamp(1, 5000)).map_err(|_| AppError::Overflow)?;
    let sql = format!(
        "{ROW_SELECT} WHERE ((t.flags & 1) <> 0 OR t.classification = 'unclassified')
         ORDER BY abs(t.amount_cents) DESC, t.posted_date, t.id LIMIT ?1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt
        .query_map([limit], row_from)?
        .collect::<Result<Vec<_>, _>>()?;
    for row in &mut rows {
        row.tags = crate::db::repo::txn::tags_of(conn, row.id)?;
    }
    Ok(rows)
}
