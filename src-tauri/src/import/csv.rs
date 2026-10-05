//! CSV parsing against a profile (ARCHITECTURE §6.1). Integer amounts, civil dates, and a
//! `Parse { row, column }` error for anything that does not fit. The whole file is parsed before
//! anything is written, so a bad row leaves nothing behind.

use std::collections::HashMap;

use chrono::{NaiveDate, NaiveDateTime};
use serde::Serialize;

use crate::dates::{format_civil, CivilDate};
use crate::error::{AppError, AppResult};
use crate::import::profile::{
    AmountSpec, DateSpec, PayeeSpec, ProfileSpec, SignConvention, TextSpec,
};
use crate::money::parse_decimal_cents;

pub const FLAG_NEEDS_REVIEW: u32 = 1;
pub const FLAG_CASH_WITHDRAWAL: u32 = 2;
pub const FLAG_PAYMENT_APP_UNKNOWN: u32 = 4;
pub const FLAG_BORROWING: u32 = 8;
pub const FLAG_SECURITIES_SALE: u32 = 16;
pub const FLAG_FEE: u32 = 32;
pub const FLAG_INTEREST: u32 = 64;

pub fn flag_bit(name: &str) -> Option<u32> {
    Some(match name {
        "needs_review" => FLAG_NEEDS_REVIEW,
        "cash_withdrawal" => FLAG_CASH_WITHDRAWAL,
        "payment_app_unknown" => FLAG_PAYMENT_APP_UNKNOWN,
        "borrowing" => FLAG_BORROWING,
        "securities_sale" => FLAG_SECURITIES_SALE,
        "fee" => FLAG_FEE,
        "interest" => FLAG_INTEREST,
        _ => return None,
    })
}

pub fn flag_names(bits: u32) -> Vec<&'static str> {
    [
        (FLAG_NEEDS_REVIEW, "needs_review"),
        (FLAG_CASH_WITHDRAWAL, "cash_withdrawal"),
        (FLAG_PAYMENT_APP_UNKNOWN, "payment_app_unknown"),
        (FLAG_BORROWING, "borrowing"),
        (FLAG_SECURITIES_SALE, "securities_sale"),
        (FLAG_FEE, "fee"),
        (FLAG_INTEREST, "interest"),
    ]
    .into_iter()
    .filter(|(bit, _)| bits & bit != 0)
    .map(|(_, name)| name)
    .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowStatus {
    Pending,
    Posted,
}

impl RowStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RowStatus::Pending => "pending",
            RowStatus::Posted => "posted",
        }
    }
}

/// One statement row after parsing and sign normalisation, before dedup. Serialised as-is into
/// the quarantine (`row_json`) and read back when the person resolves it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct ParsedRow {
    /// 1-based data row number (header and skipped preamble rows excluded).
    pub row: usize,
    pub posted_date: CivilDate,
    pub effective_date: CivilDate,
    pub amount_cents: i64,
    pub payee_raw: String,
    pub memo: String,
    pub status: RowStatus,
    pub external_id: Option<String>,
    pub balance_cents: Option<i64>,
    pub flags: u32,
    /// Set when the profile's skip rule matched; the row is reported, never inserted.
    pub skipped: Option<String>,
}

/// The statement closing a file states: the last running balance a CSV carries, or an OFX
/// `<LEDGERBAL>`. Offered to Reconcile as statement source `file`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FileClosing {
    pub date: CivilDate,
    pub cents: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParsedFile {
    pub header: Vec<String>,
    pub rows: Vec<ParsedRow>,
    /// Rows with no date and no amount (statement footers, blank lines), counted and reported.
    pub blank_rows: usize,
    pub closing: Option<FileClosing>,
}

/// Read the header row of a CSV (after the profile's preamble rows) for profile detection.
pub fn read_header(bytes: &[u8], skip_rows: usize) -> AppResult<Vec<String>> {
    let mut reader = reader(bytes);
    for (i, record) in reader.records().enumerate() {
        let record = record.map_err(|e| parse_error(i + 1, "", e.to_string()))?;
        if i < skip_rows {
            continue;
        }
        return Ok(record.iter().map(str::to_string).collect());
    }
    Err(AppError::validation("file", "the file has no header row"))
}

/// Detect the preamble length by finding the first row that matches one of the signatures.
pub fn detect_skip_rows(
    bytes: &[u8],
    candidates: &[usize],
) -> AppResult<Vec<(usize, Vec<String>)>> {
    let mut out = Vec::new();
    for &skip in candidates {
        if let Ok(header) = read_header(bytes, skip) {
            out.push((skip, header));
        }
    }
    Ok(out)
}

/// The first non-blank row after the header (for guessing formats from a sample file).
pub fn first_data_row(bytes: &[u8], skip_rows: usize) -> AppResult<Vec<String>> {
    let mut reader = reader(bytes);
    for (i, record) in reader.records().enumerate() {
        let record = record.map_err(|e| parse_error(i + 1, "", e.to_string()))?;
        if i <= skip_rows {
            continue;
        }
        if record.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        return Ok(record.iter().map(str::to_string).collect());
    }
    Ok(Vec::new())
}

fn reader(bytes: &[u8]) -> csv::Reader<&[u8]> {
    csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(bytes)
}

fn parse_error(row: usize, column: &str, message: impl Into<String>) -> AppError {
    AppError::Parse {
        row,
        column: column.to_string(),
        message: message.into(),
    }
}

struct Columns {
    index: HashMap<String, usize>,
}

impl Columns {
    fn new(header: &[String]) -> Columns {
        Columns {
            index: header
                .iter()
                .enumerate()
                .map(|(i, h)| (crate::import::profile::fold_header(h), i))
                .collect(),
        }
    }

    fn get<'a>(
        &self,
        record: &'a csv::StringRecord,
        column: &str,
        row: usize,
    ) -> AppResult<&'a str> {
        let idx = self
            .index
            .get(&crate::import::profile::fold_header(column))
            .ok_or_else(|| parse_error(row, column, "column is not in the file header"))?;
        Ok(record.get(*idx).unwrap_or(""))
    }
}

fn parse_date(spec: &DateSpec, value: &str, row: usize) -> AppResult<CivilDate> {
    let value = value.trim();
    if spec.format.contains("%H") {
        NaiveDateTime::parse_from_str(value, &spec.format)
            .map(|dt| dt.date())
            .map_err(|e| parse_error(row, &spec.column, format!("{value:?}: {e}")))
    } else {
        NaiveDate::parse_from_str(value, &spec.format)
            .map_err(|e| parse_error(row, &spec.column, format!("{value:?}: {e}")))
    }
}

fn parse_cents(column: &str, value: &str, row: usize) -> AppResult<i64> {
    parse_decimal_cents(value).map_err(|e| parse_error(row, column, format!("{value:?}: {e}")))
}

fn text(
    cols: &Columns,
    record: &csv::StringRecord,
    spec: &TextSpec,
    row: usize,
) -> AppResult<String> {
    Ok(match spec {
        TextSpec::Column { column } => cols.get(record, column, row)?.trim().to_string(),
        TextSpec::Columns { columns } => columns
            .iter()
            .map(|c| cols.get(record, c, row).map(|v| v.trim().to_string()))
            .collect::<AppResult<Vec<_>>>()?
            .into_iter()
            .filter(|v| !v.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
    })
}

/// Parse every data row. Fails on the first malformed cell with its row and column.
pub fn parse(bytes: &[u8], spec: &ProfileSpec) -> AppResult<ParsedFile> {
    let mut records = reader(bytes).into_records();
    let mut header: Option<Vec<String>> = None;
    let mut physical = 0usize;
    while header.is_none() {
        let record = records
            .next()
            .ok_or_else(|| AppError::validation("file", "the file has no header row"))?
            .map_err(|e| parse_error(physical + 1, "", e.to_string()))?;
        physical += 1;
        if physical > spec.skip_rows {
            header = Some(record.iter().map(str::to_string).collect());
        }
    }
    let header = header.unwrap_or_default();
    let cols = Columns::new(&header);

    let mut rows = Vec::new();
    let mut blank_rows = 0usize;
    let mut row = 0usize;
    for record in records {
        let record = record.map_err(|e| parse_error(row + 1, "", e.to_string()))?;
        row += 1;
        if record.iter().all(|c| c.trim().is_empty()) {
            blank_rows += 1;
            continue;
        }
        let date_raw = cols.get(&record, &spec.date.column, row).unwrap_or("");
        let amount_present = match &spec.amount {
            AmountSpec::SingleSigned { column } => !cols
                .get(&record, column, row)
                .unwrap_or("")
                .trim()
                .is_empty(),
            AmountSpec::DebitCredit { debit, credit } => {
                !cols
                    .get(&record, debit, row)
                    .unwrap_or("")
                    .trim()
                    .is_empty()
                    || !cols
                        .get(&record, credit, row)
                        .unwrap_or("")
                        .trim()
                        .is_empty()
            }
            AmountSpec::AmountWithType { column, .. } => !cols
                .get(&record, column, row)
                .unwrap_or("")
                .trim()
                .is_empty(),
        };
        if date_raw.trim().is_empty() && !amount_present {
            blank_rows += 1;
            continue;
        }

        if let Some(currency) = &spec.currency {
            let value = cols.get(&record, &currency.column, row)?.trim();
            if !value.is_empty() && !value.eq_ignore_ascii_case("USD") {
                return Err(AppError::Unsupported(format!(
                    "row {row} is in {value}; Kept v1 handles USD only"
                )));
            }
        }

        let posted_date = parse_date(&spec.date, date_raw, row)?;
        let effective_date = match &spec.effective_date {
            Some(eff) => {
                let v = cols.get(&record, &eff.column, row)?;
                if v.trim().is_empty() {
                    posted_date
                } else {
                    parse_date(eff, v, row)?
                }
            }
            None => posted_date,
        };

        let mut amount_cents = match &spec.amount {
            AmountSpec::SingleSigned { column } => {
                parse_cents(column, cols.get(&record, column, row)?, row)?
            }
            AmountSpec::DebitCredit { debit, credit } => {
                let d = cols.get(&record, debit, row)?.trim();
                let c = cols.get(&record, credit, row)?.trim();
                match (d.is_empty(), c.is_empty()) {
                    (false, true) => -parse_cents(debit, d, row)?,
                    (true, false) => parse_cents(credit, c, row)?,
                    (true, true) => {
                        return Err(parse_error(
                            row,
                            debit,
                            "neither debit nor credit is present",
                        ))
                    }
                    (false, false) => {
                        return Err(parse_error(row, debit, "both debit and credit are present"))
                    }
                }
            }
            AmountSpec::AmountWithType {
                column,
                type_column,
                debit_values,
            } => {
                let magnitude = parse_cents(column, cols.get(&record, column, row)?, row)?.abs();
                let kind = cols.get(&record, type_column, row)?.trim();
                if debit_values.iter().any(|v| v.eq_ignore_ascii_case(kind)) {
                    -magnitude
                } else {
                    magnitude
                }
            }
        };
        if spec.sign_convention == SignConvention::CardStatement {
            amount_cents = amount_cents.checked_neg().ok_or(AppError::Overflow)?;
        }

        let payee_raw = match &spec.payee {
            PayeeSpec::Column { column } => cols.get(&record, column, row)?.trim().to_string(),
            PayeeSpec::Columns { columns } => text(
                &cols,
                &record,
                &TextSpec::Columns {
                    columns: columns.clone(),
                },
                row,
            )?,
            PayeeSpec::Counterparty {
                inflow_column,
                outflow_column,
                fallback_column,
            } => {
                let primary = if amount_cents >= 0 {
                    inflow_column
                } else {
                    outflow_column
                };
                let mut value = cols.get(&record, primary, row)?.trim().to_string();
                if value.is_empty() {
                    if let Some(fallback) = fallback_column {
                        value = cols.get(&record, fallback, row)?.trim().to_string();
                    }
                }
                value
            }
        };

        let memo = match &spec.memo {
            Some(m) => text(&cols, &record, m, row)?,
            None => String::new(),
        };

        let status = match &spec.status {
            Some(s) => {
                let v = cols.get(&record, &s.column, row)?.trim();
                if s.pending_values.iter().any(|p| p.eq_ignore_ascii_case(v)) {
                    RowStatus::Pending
                } else {
                    RowStatus::Posted
                }
            }
            None => RowStatus::Posted,
        };

        let external_id = match &spec.external_id {
            Some(c) => {
                let v = cols.get(&record, &c.column, row)?.trim();
                if v.is_empty() {
                    None
                } else {
                    Some(v.to_string())
                }
            }
            None => None,
        };

        let balance_cents = match &spec.balance {
            Some(c) => {
                let v = cols.get(&record, &c.column, row)?.trim();
                if v.is_empty() {
                    None
                } else {
                    Some(parse_cents(&c.column, v, row)?)
                }
            }
            None => None,
        };

        let mut flags = 0u32;
        for name in &spec.row_flags {
            flags |= flag_bit(name).ok_or_else(|| {
                AppError::validation("spec_json", format!("unknown flag {name:?} in row_flags"))
            })?;
        }
        if let Some(by_type) = &spec.flags_by_type {
            let kind = cols.get(&record, &by_type.column, row)?.trim();
            for (value, names) in &by_type.map {
                if value.eq_ignore_ascii_case(kind) {
                    for name in names {
                        flags |= flag_bit(name).ok_or_else(|| {
                            AppError::validation(
                                "spec_json",
                                format!("unknown flag {name:?} in flags_by_type"),
                            )
                        })?;
                    }
                }
            }
        }

        let skipped = match &spec.skip_when {
            Some(rule) => {
                let v = cols.get(&record, &rule.column, row)?.trim();
                if rule
                    .not_in
                    .iter()
                    .any(|allowed| allowed.eq_ignore_ascii_case(v))
                {
                    None
                } else {
                    Some(format!("{} = {v:?}: {}", rule.column, rule.reason))
                }
            }
            None => None,
        };

        rows.push(ParsedRow {
            row,
            posted_date,
            effective_date,
            amount_cents,
            payee_raw,
            memo,
            status,
            external_id,
            balance_cents,
            flags,
            skipped,
        });
    }

    let closing = rows
        .iter()
        .filter(|r| r.balance_cents.is_some() && r.skipped.is_none())
        .max_by(|a, b| a.posted_date.cmp(&b.posted_date))
        .and_then(|r| {
            r.balance_cents.map(|cents| FileClosing {
                date: r.posted_date,
                cents,
            })
        });
    Ok(ParsedFile {
        header,
        rows,
        blank_rows,
        closing,
    })
}

/// Human-readable civil date for reports.
pub fn date_text(d: CivilDate) -> String {
    format_civil(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::profile::ProfileSpec;

    const GENERIC: &str = r#"{"header_signature":["Date","Description","Amount"],"date":{"column":"Date","format":"%Y-%m-%d"},"amount":{"kind":"single_signed","column":"Amount"},"payee":{"column":"Description"}}"#;

    #[test]
    fn parses_generic_rows_with_integer_amounts() {
        let spec = ProfileSpec::parse(GENERIC).unwrap();
        let file = parse(
            b"Date,Description,Amount\n2026-07-01,RENT,\"-2,400.00\"\n2026-07-03,ZELLE FROM M,1200.50\n\n",
            &spec,
        )
        .unwrap();
        assert_eq!(file.rows.len(), 2);
        assert_eq!(file.rows[0].amount_cents, -240_000);
        assert_eq!(file.rows[1].amount_cents, 120_050);
        assert_eq!(date_text(file.rows[1].posted_date), "2026-07-03");
        assert_eq!(file.rows[1].payee_raw, "ZELLE FROM M");
        assert_eq!(file.rows[1].status, RowStatus::Posted);
    }

    #[test]
    fn reports_the_row_and_column_of_a_bad_cell() {
        let spec = ProfileSpec::parse(GENERIC).unwrap();
        let err = parse(
            b"Date,Description,Amount\n2026-07-01,RENT,-2400\n07/02/2026,X,1\n",
            &spec,
        )
        .err()
        .unwrap();
        assert!(
            matches!(err, AppError::Parse { row: 2, ref column, .. } if column == "Date"),
            "{err:?}"
        );
        let err = parse(b"Date,Description,Amount\n2026-07-01,RENT,12.345\n", &spec)
            .err()
            .unwrap();
        assert!(
            matches!(err, AppError::Parse { row: 1, ref column, .. } if column == "Amount"),
            "{err:?}"
        );
    }

    #[test]
    fn card_statement_convention_negates_and_types_set_flags() {
        let spec = ProfileSpec::parse(
            r#"{"header_signature":["Transaction Date","Post Date","Description","Type","Amount","Status"],"date":{"column":"Post Date","format":"%m/%d/%Y"},"effective_date":{"column":"Transaction Date","format":"%m/%d/%Y"},"amount":{"kind":"single_signed","column":"Amount"},"payee":{"column":"Description"},"status":{"column":"Status","pending_values":["Pending"]},"sign_convention":"card_statement","flags_by_type":{"column":"Type","map":{"Interest":["interest"]}}}"#,
        )
        .unwrap();
        let file = parse(
            b"Transaction Date,Post Date,Description,Type,Amount,Status\n08/30/2026,09/01/2026,AMAZON.COM,Sale,62.50,Posted\n08/31/2026,08/31/2026,INTEREST CHARGE,Interest,19.44,Posted\n08/30/2026,08/30/2026,PENDING THING,Sale,5.00,Pending\n",
            &spec,
        )
        .unwrap();
        assert_eq!(file.rows[0].amount_cents, -6_250);
        assert_eq!(date_text(file.rows[0].effective_date), "2026-08-30");
        assert_eq!(date_text(file.rows[0].posted_date), "2026-09-01");
        assert_eq!(file.rows[1].flags, FLAG_INTEREST);
        assert_eq!(file.rows[2].status, RowStatus::Pending);
    }

    #[test]
    fn debit_credit_columns_and_currency_guard() {
        let spec = ProfileSpec::parse(
            r#"{"header_signature":["Transaction Date","Posted Date","Description","Debit","Credit","Balance","Currency"],"date":{"column":"Posted Date","format":"%Y-%m-%d"},"effective_date":{"column":"Transaction Date","format":"%Y-%m-%d"},"amount":{"kind":"debit_credit","debit":"Debit","credit":"Credit"},"payee":{"column":"Description"},"balance":{"column":"Balance"},"currency":{"column":"Currency"}}"#,
        )
        .unwrap();
        let file = parse(
            b"Transaction Date,Posted Date,Description,Debit,Credit,Balance,Currency\n2026-08-14,2026-08-15,ATM,163.42,,1492.71,USD\n2026-08-11,2026-08-12,DEPOSIT,,300.00,1656.13,USD\n",
            &spec,
        )
        .unwrap();
        assert_eq!(file.rows[0].amount_cents, -16_342);
        assert_eq!(file.rows[0].balance_cents, Some(149_271));
        assert_eq!(file.rows[1].amount_cents, 30_000);
        let closing = file.closing.unwrap();
        assert_eq!(
            (date_text(closing.date), closing.cents),
            ("2026-08-15".to_string(), 149_271)
        );
        let err = parse(
            b"Transaction Date,Posted Date,Description,Debit,Credit,Balance,Currency\n2026-08-14,2026-08-15,ATM,163.42,,1492.71,EUR\n",
            &spec,
        )
        .err()
        .unwrap();
        assert!(matches!(err, AppError::Unsupported(_)), "{err:?}");
    }

    #[test]
    fn preamble_counterparty_external_id_and_skip_rule() {
        let spec = ProfileSpec::parse(
            r#"{"header_signature":["ID","Datetime","Type","Status","Note","From","To","Amount (total)","Funding Source","Destination"],"skip_rows":2,"date":{"column":"Datetime","format":"%Y-%m-%dT%H:%M:%S"},"amount":{"kind":"single_signed","column":"Amount (total)"},"payee":{"inflow_column":"From","outflow_column":"To","fallback_column":"Destination"},"memo":{"column":"Note"},"external_id":{"column":"ID"},"row_flags":["payment_app_unknown","needs_review"],"skip_when":{"column":"Funding Source","not_in":["Venmo balance",""],"reason":"bank funded"}}"#,
        )
        .unwrap();
        let file = parse(
            b"Account Statement\nAccount Activity\nID,Datetime,Type,Status,Note,From,To,Amount (total),Funding Source,Destination\n1,2026-08-05T14:03:11,Payment,Complete,til payday,Chris Park,Dave,+ $600.00,Venmo balance,\n2,2026-08-06T09:15:40,Standard Transfer,Complete,,Dave,,- $600.00,Venmo balance,Northbank *1234\n3,2026-09-13T19:42:05,Payment,Complete,tix,Dave,Morgan,- $85.00,Northbank Checking,\n",
            &spec,
        )
        .unwrap();
        assert_eq!(file.rows.len(), 3);
        assert_eq!(file.rows[0].amount_cents, 60_000);
        assert_eq!(file.rows[0].payee_raw, "Chris Park");
        assert_eq!(file.rows[0].external_id.as_deref(), Some("1"));
        assert_eq!(
            file.rows[0].flags,
            FLAG_PAYMENT_APP_UNKNOWN | FLAG_NEEDS_REVIEW
        );
        assert_eq!(file.rows[0].memo, "til payday");
        assert_eq!(file.rows[1].payee_raw, "Northbank *1234");
        assert!(file.rows[2].skipped.is_some());
        assert_eq!(date_text(file.rows[2].posted_date), "2026-09-13");
    }
}
