//! Plaintext exports (ARCHITECTURE §6.6, ADR-0046): the full export (one CSV per table plus
//! `kept.json`) and the audit pack an advisor or an LLM session can read. Written only where
//! the person asks, never automatically, and never over a file that is already there. Money
//! columns become decimal strings; every figure comes from the engines, nothing is recomputed
//! here.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

use rusqlite::types::Value;
use rusqlite::Connection;
use serde::Serialize;

use crate::cash::safe;
use crate::dates::{format_civil, now_rfc3339, CivilDate};
use crate::db::migrate;
use crate::debt::strategy;
use crate::error::{AppError, AppResult};
use crate::forecast::{self, Scenario};
use crate::import::csv::flag_names;
use crate::money::to_decimal_string;
use crate::venture;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportedFile {
    pub name: String,
    pub rows: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportReport {
    pub dir: String,
    /// `full` or `audit_pack`.
    pub kind: String,
    pub files: Vec<ExportedFile>,
    pub created_at: String,
}

fn ensure_dir(dir: &Path) -> AppResult<()> {
    std::fs::create_dir_all(dir).map_err(|e| AppError::io(dir, e))
}

/// Create a file that must not exist yet: an export never overwrites.
fn create_new(dir: &Path, name: &str) -> AppResult<File> {
    let path = dir.join(name);
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                AppError::Conflict(format!(
                    "{} already exists; choose an empty folder",
                    path.display()
                ))
            } else {
                AppError::io(&path, e)
            }
        })
}

fn write_text(dir: &Path, name: &str, text: &str) -> AppResult<()> {
    let mut f = create_new(dir, name)?;
    f.write_all(text.as_bytes())
        .map_err(|e| AppError::io(dir.join(name), e))
}

fn csv_error(dir: &Path, name: &str, e: csv::Error) -> AppError {
    AppError::Io {
        path: dir.join(name).display().to_string(),
        source: std::io::Error::other(e.to_string()),
    }
}

/// A CSV file with a header and rows of strings; returns the number of data rows.
fn write_csv(dir: &Path, name: &str, header: &[&str], rows: &[Vec<String>]) -> AppResult<usize> {
    let file = create_new(dir, name)?;
    let mut w = csv::Writer::from_writer(file);
    w.write_record(header)
        .map_err(|e| csv_error(dir, name, e))?;
    for row in rows {
        w.write_record(row).map_err(|e| csv_error(dir, name, e))?;
    }
    w.flush().map_err(|e| AppError::io(dir.join(name), e))?;
    Ok(rows.len())
}

/// Column `x_cents` is exported as `x` holding a decimal string; everything else as stored.
fn export_column(name: &str) -> String {
    name.strip_suffix("_cents")
        .map_or_else(|| name.to_string(), str::to_string)
}

fn cell(column: &str, value: &Value) -> AppResult<String> {
    Ok(match value {
        Value::Null => String::new(),
        Value::Integer(i) if column.ends_with("_cents") => to_decimal_string(*i),
        Value::Integer(i) => i.to_string(),
        Value::Text(t) => t.clone(),
        Value::Blob(b) => hex::encode(b),
        Value::Real(_) => {
            return Err(AppError::Internal(format!(
                "column {column} holds a REAL value; the schema bans them"
            )))
        }
    })
}

fn json_cell(column: &str, value: &Value) -> AppResult<serde_json::Value> {
    Ok(match value {
        Value::Null => serde_json::Value::Null,
        Value::Integer(i) if column.ends_with("_cents") => {
            serde_json::Value::String(to_decimal_string(*i))
        }
        Value::Integer(i) => serde_json::Value::from(*i),
        Value::Text(t) => serde_json::Value::String(t.clone()),
        Value::Blob(b) => serde_json::Value::String(hex::encode(b)),
        Value::Real(_) => {
            return Err(AppError::Internal(format!(
                "column {column} holds a REAL value; the schema bans them"
            )))
        }
    })
}

fn user_tables(conn: &Connection) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let names = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(names)
}

fn columns(conn: &Connection, table: &str) -> AppResult<Vec<String>> {
    // table names come from sqlite_master, never from input
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
    let names = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(names)
}

/// One CSV per table plus `kept.json` holding every table, with money as decimal strings.
pub fn full(conn: &Connection, dir: &Path, app_version: &str) -> AppResult<ExportReport> {
    ensure_dir(dir)?;
    let created_at = now_rfc3339();
    let mut files = Vec::new();
    let mut json_tables = serde_json::Map::new();
    for table in user_tables(conn)? {
        let cols = columns(conn, &table)?;
        let header: Vec<String> = cols.iter().map(|c| export_column(c)).collect();
        let header_refs: Vec<&str> = header.iter().map(String::as_str).collect();
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{table}\" ORDER BY rowid"))?;
        let mut rows_csv: Vec<Vec<String>> = Vec::new();
        let mut rows_json: Vec<serde_json::Value> = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let mut line = Vec::with_capacity(cols.len());
            let mut object = serde_json::Map::with_capacity(cols.len());
            for (i, col) in cols.iter().enumerate() {
                let value: Value = row.get(i)?;
                line.push(cell(col, &value)?);
                object.insert(export_column(col), json_cell(col, &value)?);
            }
            rows_csv.push(line);
            rows_json.push(serde_json::Value::Object(object));
        }
        let name = format!("{table}.csv");
        let n = write_csv(dir, &name, &header_refs, &rows_csv)?;
        files.push(ExportedFile { name, rows: n });
        json_tables.insert(table, serde_json::Value::Array(rows_json));
    }
    let doc = serde_json::json!({
        "exported_at": created_at,
        "app_version": app_version,
        "schema_version": migrate::current_version(conn)?,
        "money": "decimal strings in USD; a column stored as *_cents is exported without the suffix",
        "tables": json_tables,
    });
    let text = serde_json::to_string_pretty(&doc)?;
    write_text(dir, "kept.json", &text)?;
    files.push(ExportedFile {
        name: "kept.json".into(),
        rows: json_tables_len(&doc),
    });
    tracing::info!(dir = %dir.display(), files = files.len(), "full export written");
    Ok(ExportReport {
        dir: dir.display().to_string(),
        kind: "full".into(),
        files,
        created_at,
    })
}

fn json_tables_len(doc: &serde_json::Value) -> usize {
    doc["tables"].as_object().map_or(0, serde_json::Map::len)
}

const LEDGER_SQL: &str = "SELECT t.id, a.name, t.posted_date, t.effective_date, t.amount_cents, t.payee_raw, t.memo,
        COALESCE(p.name || ' › ', '') || COALESCE(c.name, ''), t.status, t.flags, COALESCE(v.name, ''),
        t.transfer_link_id, t.refund_link_id, COALESCE(t.external_id, ''), t.classification
    FROM txn_leaf t
    JOIN account a ON a.id = t.account_id
    LEFT JOIN category c ON c.id = t.category_id
    LEFT JOIN category p ON p.id = c.parent_id
    LEFT JOIN venture v ON v.id = t.venture_id
    ORDER BY t.posted_date, t.id";

const RECON_SQL: &str = "SELECT a.name, r.period_start, r.period_end, r.opening_cents, r.computed_closing_cents,
        r.statement_closing_cents, r.difference_cents, r.status, r.statement_source, COALESCE(r.balanced_at, '')
    FROM reconciliation r JOIN account a ON a.id = r.account_id
    ORDER BY a.name, r.period_end";

fn opt_id(v: Option<i64>) -> String {
    v.map(|i| i.to_string()).unwrap_or_default()
}

const README: &str = "# Kept audit pack

Every file is plaintext and holds the figures Kept's core produced; nothing here is recomputed.
Amounts are decimal US dollars. Sign is the account's point of view: an outflow from a checking
account is negative, a card purchase is negative on the card, a payment to the card is positive
on the card and negative on the account that paid it. Dates are civil YYYY-MM-DD.

- ledger.csv — every leaf ledger row (a split parent is never counted) with its account,
  category, status, flags, venture and link ids. Transfers, card payments and loan repayments
  carry a transfer_link_id: the pair nets to zero and is not spending.
- reconciliation.csv — every statement period entered: opening, computed closing, statement
  closing, difference (zero means balanced) and whether the closing came from a file.
- safe_to_spend.json — the hero as of the export day: available, earmarks, obligations before
  the next confirmed income, timing buffer, the row ids behind each term, what was excluded and
  why, and the trust report. safe = available - earmarks - obligations - buffer.
- forecast.json — the baseline daily projection for the next 90 days with every event.
- debt_schedule.csv — the avalanche payoff schedule at minimums only (no extra), one row per
  debt per period: opening, interest, minimum, payment, closing.
- venture_rollup.csv — one row per venture: the five buckets over the trailing window,
  operating cash flow, cap used and utilization in basis points, milestone countdown, alerts.
";

/// The six files plus a README, as of `today`.
pub fn audit_pack(
    conn: &Connection,
    dir: &Path,
    today: CivilDate,
    app_version: &str,
) -> AppResult<ExportReport> {
    ensure_dir(dir)?;
    let created_at = now_rfc3339();
    let mut files = Vec::new();

    let mut stmt = conn.prepare(LEDGER_SQL)?;
    let ledger: Vec<Vec<String>> = stmt
        .query_map([], |r| {
            let flags: i64 = r.get(9)?;
            Ok(vec![
                r.get::<_, i64>(0)?.to_string(),
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                to_decimal_string(r.get::<_, i64>(4)?),
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
                flag_names(u32::try_from(flags).unwrap_or(0)).join(" "),
                r.get::<_, String>(10)?,
                opt_id(r.get::<_, Option<i64>>(11)?),
                opt_id(r.get::<_, Option<i64>>(12)?),
                r.get::<_, String>(13)?,
                r.get::<_, String>(14)?,
            ])
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let n = write_csv(
        dir,
        "ledger.csv",
        &[
            "id",
            "account",
            "posted_date",
            "effective_date",
            "amount",
            "payee",
            "memo",
            "category",
            "status",
            "flags",
            "venture",
            "transfer_link_id",
            "refund_link_id",
            "external_id",
            "classification",
        ],
        &ledger,
    )?;
    files.push(ExportedFile {
        name: "ledger.csv".into(),
        rows: n,
    });

    let mut stmt = conn.prepare(RECON_SQL)?;
    let recon: Vec<Vec<String>> = stmt
        .query_map([], |r| {
            Ok(vec![
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                to_decimal_string(r.get::<_, i64>(3)?),
                to_decimal_string(r.get::<_, i64>(4)?),
                to_decimal_string(r.get::<_, i64>(5)?),
                to_decimal_string(r.get::<_, i64>(6)?),
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
                r.get::<_, String>(9)?,
            ])
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let n = write_csv(
        dir,
        "reconciliation.csv",
        &[
            "account",
            "period_start",
            "period_end",
            "opening",
            "computed_closing",
            "statement_closing",
            "difference",
            "status",
            "statement_source",
            "balanced_at",
        ],
        &recon,
    )?;
    files.push(ExportedFile {
        name: "reconciliation.csv".into(),
        rows: n,
    });

    let hero = safe::safe_to_spend(conn, today)?;
    write_text(
        dir,
        "safe_to_spend.json",
        &serde_json::to_string_pretty(&hero)?,
    )?;
    files.push(ExportedFile {
        name: "safe_to_spend.json".into(),
        rows: 1,
    });

    let baseline = forecast::run(conn, today, &Scenario::default())?;
    write_text(
        dir,
        "forecast.json",
        &serde_json::to_string_pretty(&baseline)?,
    )?;
    files.push(ExportedFile {
        name: "forecast.json".into(),
        rows: baseline.days.len(),
    });

    let run = strategy::run(conn, today, "avalanche", 0)?;
    let mut schedule: Vec<Vec<String>> = Vec::new();
    for d in &run.debts {
        for p in &d.periods {
            schedule.push(vec![
                d.name.clone(),
                d.kind.clone(),
                if d.informal { "yes" } else { "no" }.to_string(),
                p.period.to_string(),
                p.start.clone(),
                p.end.clone(),
                to_decimal_string(p.opening_cents),
                to_decimal_string(p.interest_cents),
                to_decimal_string(p.minimum_cents),
                to_decimal_string(p.payment_cents),
                to_decimal_string(p.closing_cents),
            ]);
        }
    }
    let n = write_csv(
        dir,
        "debt_schedule.csv",
        &[
            "debt", "kind", "informal", "period", "start", "end", "opening", "interest", "minimum",
            "payment", "closing",
        ],
        &schedule,
    )?;
    files.push(ExportedFile {
        name: "debt_schedule.csv".into(),
        rows: n,
    });

    let summary = venture::summary(conn, today)?;
    let rollups: Vec<Vec<String>> = summary
        .ventures
        .iter()
        .map(|r| {
            vec![
                r.venture.name.clone(),
                r.venture.status.clone(),
                r.as_of.clone(),
                r.window_start.clone(),
                to_decimal_string(r.venture.cash_cap_cents),
                to_decimal_string(r.customer_revenue.cents),
                to_decimal_string(r.operating_expense.cents),
                to_decimal_string(r.owner_contribution.cents),
                to_decimal_string(r.financing.cents),
                to_decimal_string(r.withdrawal.cents),
                to_decimal_string(r.operating_cash_flow_cents),
                to_decimal_string(r.cap_used_cents),
                to_decimal_string(r.cap_remaining_cents),
                r.cap_utilization_bps.to_string(),
                r.milestone_days.map(|d| d.to_string()).unwrap_or_default(),
                r.alerts.join(" "),
            ]
        })
        .collect();
    let n = write_csv(
        dir,
        "venture_rollup.csv",
        &[
            "venture",
            "status",
            "as_of",
            "window_start",
            "cash_cap",
            "customer_revenue",
            "operating_expense",
            "owner_contribution",
            "financing",
            "withdrawal",
            "operating_cash_flow",
            "cap_used",
            "cap_remaining",
            "cap_utilization_bps",
            "milestone_days",
            "alerts",
        ],
        &rollups,
    )?;
    files.push(ExportedFile {
        name: "venture_rollup.csv".into(),
        rows: n,
    });

    let readme = format!(
        "{README}\nExported {created_at} by Kept {app_version} as of {}.\n",
        format_civil(today)
    );
    write_text(dir, "README.md", &readme)?;
    files.push(ExportedFile {
        name: "README.md".into(),
        rows: 1,
    });
    tracing::info!(dir = %dir.display(), "audit pack written");
    Ok(ExportReport {
        dir: dir.display().to_string(),
        kind: "audit_pack".into(),
        files,
        created_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cents_columns_become_decimal_strings_without_the_suffix() {
        assert_eq!(export_column("amount_cents"), "amount");
        assert_eq!(export_column("posted_date"), "posted_date");
        assert_eq!(
            cell("amount_cents", &Value::Integer(-240_000)).unwrap(),
            "-2400.00"
        );
        assert_eq!(cell("rows", &Value::Integer(7)).unwrap(), "7");
        assert_eq!(cell("memo", &Value::Null).unwrap(), "");
        assert!(cell("x", &Value::Real(1.5)).is_err());
    }
}
