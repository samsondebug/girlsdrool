//! Institution import profiles (ARCHITECTURE §6.1): how one statement layout maps onto ledger
//! rows. Profiles are data (`import_profile.spec_json`), so a new bank is a row, not a release.
//! CSV profiles are per institution and editable in Settings; the OFX/QFX profile is one system
//! row because the OFX standard fixes the mapping (ADR-0046).

use std::collections::BTreeMap;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::dates::now_rfc3339;
use crate::db::audit::{self, Action, CommandRecord};
use crate::error::{AppError, AppResult};
use crate::import::csv::flag_bit;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DateSpec {
    pub column: String,
    /// chrono format; a format containing `%H` is parsed as a datetime and reduced to its date.
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AmountSpec {
    /// One signed column, e.g. `-1,234.56`.
    SingleSigned { column: String },
    /// Two positive columns; debit is an outflow.
    DebitCredit { debit: String, credit: String },
    /// One unsigned column plus a type column whose listed values mean outflow.
    AmountWithType {
        column: String,
        type_column: String,
        debit_values: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum PayeeSpec {
    /// Counterparty depends on direction (payment apps): `From` for inflows, `To` for outflows,
    /// with a fallback column when the chosen one is empty.
    Counterparty {
        inflow_column: String,
        outflow_column: String,
        #[serde(default)]
        fallback_column: Option<String>,
    },
    Column {
        column: String,
    },
    Columns {
        columns: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum TextSpec {
    Column { column: String },
    Columns { columns: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatusSpec {
    pub column: String,
    pub pending_values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ColumnSpec {
    pub column: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SignConvention {
    /// Amounts are already from the account's point of view.
    #[default]
    AccountPov,
    /// Card statements: purchases positive, payments negative; the profile negates.
    CardStatement,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlagsByType {
    pub column: String,
    /// Type value (compared case-insensitively) → flag names (ARCHITECTURE §3.4).
    pub map: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkipRule {
    pub column: String,
    /// Rows whose value is not in this list are skipped (reported, never silent).
    pub not_in: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileSpec {
    pub header_signature: Vec<String>,
    #[serde(default)]
    pub skip_rows: usize,
    pub date: DateSpec,
    #[serde(default)]
    pub effective_date: Option<DateSpec>,
    pub amount: AmountSpec,
    pub payee: PayeeSpec,
    #[serde(default)]
    pub memo: Option<TextSpec>,
    #[serde(default)]
    pub status: Option<StatusSpec>,
    #[serde(default)]
    pub external_id: Option<ColumnSpec>,
    #[serde(default)]
    pub balance: Option<ColumnSpec>,
    #[serde(default)]
    pub currency: Option<ColumnSpec>,
    #[serde(default)]
    pub sign_convention: SignConvention,
    #[serde(default)]
    pub flags_by_type: Option<FlagsByType>,
    /// Flags applied to every row (payment apps: `payment_app_unknown`, `needs_review`).
    #[serde(default)]
    pub row_flags: Vec<String>,
    #[serde(default)]
    pub skip_when: Option<SkipRule>,
}

impl ProfileSpec {
    pub fn parse(json: &str) -> AppResult<ProfileSpec> {
        serde_json::from_str(json)
            .map_err(|e| AppError::validation("spec_json", format!("invalid profile spec: {e}")))
    }
}

/// The OFX/QFX mapping: the standard names every field, so the only institution-specific knob
/// is which `TRNTYPE` values set which row flags (ARCHITECTURE §3.4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OfxSpec {
    /// `TRNTYPE` (compared case-insensitively) → flag names.
    #[serde(default)]
    pub flags_by_trntype: BTreeMap<String, Vec<String>>,
}

impl OfxSpec {
    pub fn parse(json: &str) -> AppResult<OfxSpec> {
        let spec: OfxSpec = serde_json::from_str(json)
            .map_err(|e| AppError::validation("spec_json", format!("invalid OFX spec: {e}")))?;
        for names in spec.flags_by_trntype.values() {
            for name in names {
                flag_bit(name).ok_or_else(|| {
                    AppError::validation(
                        "spec_json",
                        format!("unknown flag {name:?} in flags_by_trntype"),
                    )
                })?;
            }
        }
        Ok(spec)
    }
}

/// One profile's mapping, by file format. Serialised untagged: the `format` column beside it
/// says which it is.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Spec {
    Csv(Box<ProfileSpec>),
    Ofx(OfxSpec),
}

impl Spec {
    pub fn parse(format: &str, json: &str) -> AppResult<Spec> {
        match format {
            "csv" => Ok(Spec::Csv(Box::new(ProfileSpec::parse(json)?))),
            "ofx" => Ok(Spec::Ofx(OfxSpec::parse(json)?)),
            other => Err(AppError::validation(
                "format",
                format!("unknown profile format {other:?}"),
            )),
        }
    }

    pub fn format(&self) -> &'static str {
        match self {
            Spec::Csv(_) => "csv",
            Spec::Ofx(_) => "ofx",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Profile {
    pub id: i64,
    pub name: String,
    pub institution: String,
    pub format: String,
    pub is_system: bool,
    pub spec: Spec,
}

impl Profile {
    pub fn csv(&self) -> Option<&ProfileSpec> {
        match &self.spec {
            Spec::Csv(spec) => Some(spec),
            Spec::Ofx(_) => None,
        }
    }

    pub fn ofx(&self) -> Option<&OfxSpec> {
        match &self.spec {
            Spec::Ofx(spec) => Some(spec),
            Spec::Csv(_) => None,
        }
    }
}

const COLS: &str = "id, name, institution, format, is_system, spec_json";

fn read_profile(
    r: &rusqlite::Row<'_>,
) -> rusqlite::Result<(i64, String, String, String, i64, String)> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
    ))
}

fn build(row: (i64, String, String, String, i64, String)) -> AppResult<Profile> {
    let (id, name, institution, format, is_system, spec_json) = row;
    Ok(Profile {
        id,
        name,
        institution,
        is_system: is_system == 1,
        spec: Spec::parse(&format, &spec_json)?,
        format,
    })
}

pub fn list(conn: &Connection) -> AppResult<Vec<Profile>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM import_profile ORDER BY name"))?;
    let rows = stmt.query_map([], read_profile)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(build(row?)?);
    }
    Ok(out)
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Profile> {
    conn.query_row(
        &format!("SELECT {COLS} FROM import_profile WHERE id = ?1"),
        [id],
        read_profile,
    )
    .optional()?
    .ok_or(AppError::NotFound {
        entity: "import_profile",
        id,
    })
    .and_then(build)
}

/// The one profile for OFX/QFX files (migration 0005), if the database has it.
pub fn ofx_profile(conn: &Connection) -> AppResult<Option<Profile>> {
    Ok(list(conn)?.into_iter().find(|p| p.format == "ofx"))
}

/// A CSV profile as the editor submits it (ADR-0046). The format is always `csv`: the OFX
/// profile is a system row and is never created or edited by hand.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileInput {
    pub name: String,
    pub institution: String,
    pub spec: ProfileSpec,
}

fn signature_has(spec: &ProfileSpec, column: &str) -> bool {
    let wanted = fold_header(column);
    spec.header_signature
        .iter()
        .any(|h| fold_header(h) == wanted)
}

fn need_column(spec: &ProfileSpec, field: &str, column: &str) -> AppResult<()> {
    if column.trim().is_empty() {
        return Err(AppError::validation(field, "a column name is required"));
    }
    if !signature_has(spec, column) {
        return Err(AppError::validation(
            field,
            format!("column {column:?} is not in the header signature"),
        ));
    }
    Ok(())
}

/// Every column the mapping names must be in the header signature, every flag must exist, and
/// a date needs a format: the same checks the parser would fail on, made before anything is
/// stored.
pub fn validate_spec(spec: &ProfileSpec) -> AppResult<()> {
    if spec.header_signature.is_empty() {
        return Err(AppError::validation(
            "header_signature",
            "the header signature is empty; read it from a sample file",
        ));
    }
    need_column(spec, "date.column", &spec.date.column)?;
    if spec.date.format.trim().is_empty() {
        return Err(AppError::validation(
            "date.format",
            "a chrono date format is required, e.g. %Y-%m-%d",
        ));
    }
    if let Some(eff) = &spec.effective_date {
        need_column(spec, "effective_date.column", &eff.column)?;
        if eff.format.trim().is_empty() {
            return Err(AppError::validation(
                "effective_date.format",
                "a chrono date format is required",
            ));
        }
    }
    match &spec.amount {
        AmountSpec::SingleSigned { column } => need_column(spec, "amount.column", column)?,
        AmountSpec::DebitCredit { debit, credit } => {
            need_column(spec, "amount.debit", debit)?;
            need_column(spec, "amount.credit", credit)?;
        }
        AmountSpec::AmountWithType {
            column,
            type_column,
            debit_values,
        } => {
            need_column(spec, "amount.column", column)?;
            need_column(spec, "amount.type_column", type_column)?;
            if debit_values.is_empty() {
                return Err(AppError::validation(
                    "amount.debit_values",
                    "list the type values that mean an outflow",
                ));
            }
        }
    }
    match &spec.payee {
        PayeeSpec::Column { column } => need_column(spec, "payee.column", column)?,
        PayeeSpec::Columns { columns } => {
            if columns.is_empty() {
                return Err(AppError::validation(
                    "payee.columns",
                    "name at least one column",
                ));
            }
            for c in columns {
                need_column(spec, "payee.columns", c)?;
            }
        }
        PayeeSpec::Counterparty {
            inflow_column,
            outflow_column,
            fallback_column,
        } => {
            need_column(spec, "payee.inflow_column", inflow_column)?;
            need_column(spec, "payee.outflow_column", outflow_column)?;
            if let Some(f) = fallback_column {
                need_column(spec, "payee.fallback_column", f)?;
            }
        }
    }
    match &spec.memo {
        Some(TextSpec::Column { column }) => need_column(spec, "memo.column", column)?,
        Some(TextSpec::Columns { columns }) => {
            for c in columns {
                need_column(spec, "memo.columns", c)?;
            }
        }
        None => {}
    }
    if let Some(st) = &spec.status {
        need_column(spec, "status.column", &st.column)?;
    }
    for (field, col) in [
        ("external_id.column", &spec.external_id),
        ("balance.column", &spec.balance),
        ("currency.column", &spec.currency),
    ] {
        if let Some(c) = col {
            need_column(spec, field, &c.column)?;
        }
    }
    if let Some(by_type) = &spec.flags_by_type {
        need_column(spec, "flags_by_type.column", &by_type.column)?;
        for names in by_type.map.values() {
            for name in names {
                flag_bit(name).ok_or_else(|| {
                    AppError::validation("flags_by_type", format!("unknown flag {name:?}"))
                })?;
            }
        }
    }
    for name in &spec.row_flags {
        flag_bit(name)
            .ok_or_else(|| AppError::validation("row_flags", format!("unknown flag {name:?}")))?;
    }
    if let Some(rule) = &spec.skip_when {
        need_column(spec, "skip_when.column", &rule.column)?;
    }
    Ok(())
}

fn validate_input(input: &ProfileInput) -> AppResult<()> {
    if input.name.trim().is_empty() {
        return Err(AppError::validation("name", "a profile name is required"));
    }
    validate_spec(&input.spec)
}

fn name_taken(conn: &Connection, name: &str, except: Option<i64>) -> AppResult<bool> {
    let hit: Option<i64> = conn
        .query_row(
            "SELECT id FROM import_profile WHERE name = ?1 AND id IS NOT ?2",
            params![name, except],
            |r| r.get(0),
        )
        .optional()?;
    Ok(hit.is_some())
}

/// Create a CSV profile. One audit row; the mapping is validated first.
pub fn create(conn: &Connection, cmd: &CommandRecord, input: &ProfileInput) -> AppResult<Profile> {
    validate_input(input)?;
    let name = input.name.trim();
    if name_taken(conn, name, None)? {
        return Err(AppError::Conflict(format!(
            "a profile named {name:?} already exists"
        )));
    }
    let now = now_rfc3339();
    conn.execute(
        "INSERT INTO import_profile (name, institution, format, spec_json, is_system, created_at, updated_at)
         VALUES (?1, ?2, 'csv', ?3, 0, ?4, ?4)",
        params![
            name,
            input.institution.trim(),
            serde_json::to_string(&input.spec)?,
            now
        ],
    )?;
    let created = get(conn, conn.last_insert_rowid())?;
    audit::record(
        conn,
        cmd,
        "import_profile",
        created.id,
        Action::Insert,
        None,
        Some(&serde_json::to_value(&created)?),
    )?;
    Ok(created)
}

/// Replace a CSV profile's name, institution and mapping. System profiles are read-only.
pub fn update(
    conn: &Connection,
    cmd: &CommandRecord,
    id: i64,
    input: &ProfileInput,
) -> AppResult<Profile> {
    let before = get(conn, id)?;
    if before.is_system {
        return Err(AppError::Conflict(format!(
            "{} is a built-in profile; copy it into a new profile to change the mapping",
            before.name
        )));
    }
    if before.format != "csv" {
        return Err(AppError::Conflict(format!(
            "{} is an {} profile; only CSV mappings are edited",
            before.name, before.format
        )));
    }
    validate_input(input)?;
    let name = input.name.trim();
    if name_taken(conn, name, Some(id))? {
        return Err(AppError::Conflict(format!(
            "a profile named {name:?} already exists"
        )));
    }
    conn.execute(
        "UPDATE import_profile SET name = ?1, institution = ?2, spec_json = ?3, updated_at = ?4 WHERE id = ?5",
        params![
            name,
            input.institution.trim(),
            serde_json::to_string(&input.spec)?,
            now_rfc3339(),
            id
        ],
    )?;
    let after = get(conn, id)?;
    audit::record(
        conn,
        cmd,
        "import_profile",
        id,
        Action::Update,
        Some(&serde_json::to_value(&before)?),
        Some(&serde_json::to_value(&after)?),
    )?;
    Ok(after)
}

/// Delete a CSV profile nothing refers to. A profile an import batch used stays: the batch's
/// report names it.
pub fn delete(conn: &Connection, cmd: &CommandRecord, id: i64) -> AppResult<()> {
    let before = get(conn, id)?;
    if before.is_system {
        return Err(AppError::Conflict(format!(
            "{} is a built-in profile and cannot be deleted",
            before.name
        )));
    }
    let batches: i64 = conn.query_row(
        "SELECT count(*) FROM import_batch WHERE profile_id = ?1",
        [id],
        |r| r.get(0),
    )?;
    if batches > 0 {
        return Err(AppError::Conflict(format!(
            "{} was used by {batches} import batch(es); it stays so their reports keep their meaning",
            before.name
        )));
    }
    conn.execute("DELETE FROM import_profile WHERE id = ?1", [id])?;
    audit::record(
        conn,
        cmd,
        "import_profile",
        id,
        Action::Delete,
        Some(&serde_json::to_value(&before)?),
        None,
    )?;
    Ok(())
}

/// A mapping proposed from a sample file: the detected header and a first guess at the columns,
/// for the person to correct and test (ADR-0046). Nothing is stored.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Draft {
    pub skip_rows: usize,
    pub header: Vec<String>,
    pub spec: ProfileSpec,
}

const HEADER_WORDS: [&str; 9] = [
    "date",
    "amount",
    "description",
    "payee",
    "debit",
    "credit",
    "memo",
    "balance",
    "name",
];
const SKIP_CANDIDATES: [usize; 11] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

fn find_header(header: &[String], predicate: impl Fn(&str) -> bool) -> Option<&String> {
    header.iter().find(|h| predicate(&fold_header(h)))
}

fn guess_date_format(sample: &str) -> &'static str {
    let s = sample.trim();
    let digits = |range: std::ops::Range<usize>| {
        s.get(range)
            .is_some_and(|x| x.chars().all(|c| c.is_ascii_digit()))
    };
    if digits(0..4)
        && s.get(4..5) == Some("-")
        && digits(5..7)
        && s.get(7..8) == Some("-")
        && digits(8..10)
    {
        if s.get(10..11) == Some("T") {
            return "%Y-%m-%dT%H:%M:%S";
        }
        if s.get(10..11) == Some(" ") {
            return "%Y-%m-%d %H:%M:%S";
        }
        return "%Y-%m-%d";
    }
    if s.len() == 8 && digits(0..8) {
        return "%Y%m%d";
    }
    if s.contains('/') {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() == 3 && parts[2].len() == 4 {
            return "%m/%d/%Y";
        }
        if parts.len() == 3 && parts[0].len() == 4 {
            return "%Y/%m/%d";
        }
        return "%m/%d/%y";
    }
    "%Y-%m-%d"
}

/// Guess a mapping from a sample CSV: the first plausible header row (up to ten preamble rows),
/// then columns by name. Every guess is a starting point the editor shows for correction.
pub fn draft_from_sample(bytes: &[u8]) -> AppResult<Draft> {
    let candidates = crate::import::csv::detect_skip_rows(bytes, &SKIP_CANDIDATES)?;
    let (skip_rows, header) = candidates
        .iter()
        .find(|(_, header)| {
            header.len() >= 2
                && header.iter().any(|h| {
                    let f = fold_header(h);
                    HEADER_WORDS.iter().any(|w| f.contains(w))
                })
        })
        .or_else(|| candidates.first())
        .cloned()
        .ok_or_else(|| AppError::validation("file", "the file has no header row"))?;
    let first_row = crate::import::csv::first_data_row(bytes, skip_rows)?;
    let cell = |column: &String| -> String {
        header
            .iter()
            .position(|h| h == column)
            .and_then(|i| first_row.get(i))
            .cloned()
            .unwrap_or_default()
    };

    let date = find_header(&header, |f| f.contains("post") && f.contains("date"))
        .or_else(|| find_header(&header, |f| f == "date"))
        .or_else(|| {
            find_header(&header, |f| {
                f.contains("date") && !f.contains("transaction")
            })
        })
        .or_else(|| find_header(&header, |f| f.contains("date")))
        .ok_or_else(|| {
            AppError::validation("date.column", "no column looks like a date; map it by hand")
        })?
        .clone();
    let effective_date = find_header(&header, |f| {
        (f.contains("transaction date") || f.contains("trans date") || f.contains("trans. date"))
            && f != fold_header(&date)
    })
    .map(|c| DateSpec {
        column: c.clone(),
        format: guess_date_format(&cell(c)).to_string(),
    });
    let debit = find_header(&header, |f| f == "debit" || f.starts_with("debit"));
    let credit = find_header(&header, |f| f == "credit" || f.starts_with("credit"));
    let amount = match (debit, credit) {
        (Some(d), Some(c)) => AmountSpec::DebitCredit {
            debit: d.clone(),
            credit: c.clone(),
        },
        _ => {
            let column = find_header(&header, |f| f == "amount")
                .or_else(|| find_header(&header, |f| f.contains("amount")))
                .ok_or_else(|| {
                    AppError::validation("amount", "no column looks like an amount; map it by hand")
                })?;
            AmountSpec::SingleSigned {
                column: column.clone(),
            }
        }
    };
    let payee = find_header(&header, |f| f.contains("description"))
        .or_else(|| find_header(&header, |f| f.contains("payee")))
        .or_else(|| find_header(&header, |f| f.contains("merchant")))
        .or_else(|| find_header(&header, |f| f == "name"))
        .or_else(|| find_header(&header, |f| f.contains("activity")))
        .or_else(|| find_header(&header, |f| f.contains("memo") || f.contains("note")))
        .ok_or_else(|| {
            AppError::validation(
                "payee",
                "no column looks like a payee or description; map it by hand",
            )
        })?
        .clone();
    let memo = find_header(&header, |f| {
        (f.contains("memo") || f.contains("note")) && f != fold_header(&payee)
    })
    .map(|c| TextSpec::Column { column: c.clone() });
    let status = find_header(&header, |f| f == "status").map(|c| StatusSpec {
        column: c.clone(),
        pending_values: vec!["Pending".to_string()],
    });
    let column_spec = |pred: &dyn Fn(&str) -> bool| {
        find_header(&header, pred).map(|c| ColumnSpec { column: c.clone() })
    };
    let external_id = column_spec(&|f| {
        f == "id" || f == "transaction id" || f == "reference" || f == "reference number"
    });
    let balance =
        column_spec(&|f| f.contains("balance") || f.ends_with(" bal.") || f.ends_with(" bal"));
    let currency = column_spec(&|f| f == "currency");

    let spec = ProfileSpec {
        header_signature: header.clone(),
        skip_rows,
        date: DateSpec {
            format: guess_date_format(&cell(&date)).to_string(),
            column: date,
        },
        effective_date,
        amount,
        payee: PayeeSpec::Column { column: payee },
        memo,
        status,
        external_id,
        balance,
        currency,
        sign_convention: SignConvention::AccountPov,
        flags_by_type: None,
        row_flags: Vec::new(),
        skip_when: None,
    };
    Ok(Draft {
        skip_rows,
        header,
        spec,
    })
}

/// Normalise a header cell for signature comparison: trim, collapse whitespace, lowercase.
pub fn fold_header(cell: &str) -> String {
    cell.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Profiles whose signature matches the header row exactly (after folding).
pub fn matching<'a>(profiles: &'a [Profile], header: &[String]) -> Vec<&'a Profile> {
    let folded: Vec<String> = header.iter().map(|h| fold_header(h)).collect();
    profiles
        .iter()
        .filter(|p| {
            p.csv().is_some_and(|spec| {
                spec.header_signature.len() == folded.len()
                    && spec
                        .header_signature
                        .iter()
                        .map(|h| fold_header(h))
                        .eq(folded.iter().cloned())
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_profile_spec_parses_with_untagged_payee() {
        let spec = ProfileSpec::parse(
            r#"{"header_signature":["Date","Description","Amount"],"date":{"column":"Date","format":"%Y-%m-%d"},"amount":{"kind":"single_signed","column":"Amount"},"payee":{"column":"Description"}}"#,
        )
        .unwrap();
        assert_eq!(
            spec.payee,
            PayeeSpec::Column {
                column: "Description".into()
            }
        );
        assert_eq!(spec.sign_convention, SignConvention::AccountPov);
        assert_eq!(spec.skip_rows, 0);
    }

    #[test]
    fn header_folding_ignores_case_and_spacing() {
        assert_eq!(fold_header("  Running   Bal. "), "running bal.");
    }

    #[test]
    fn a_mapping_that_names_a_column_outside_the_signature_is_refused() {
        let mut spec = ProfileSpec::parse(
            r#"{"header_signature":["Date","Description","Amount"],"date":{"column":"Date","format":"%Y-%m-%d"},"amount":{"kind":"single_signed","column":"Amount"},"payee":{"column":"Description"}}"#,
        )
        .unwrap();
        validate_spec(&spec).unwrap();
        spec.balance = Some(ColumnSpec {
            column: "Balance".into(),
        });
        let err = validate_spec(&spec).err().unwrap();
        assert!(
            matches!(err, AppError::Validation { ref field, .. } if field == "balance.column"),
            "{err:?}"
        );
        spec.balance = None;
        spec.row_flags = vec!["bogus".into()];
        assert!(validate_spec(&spec).is_err());
    }

    /// Preamble rows are records, as the parser counts them: the CSV reader skips blank lines.
    #[test]
    fn a_draft_reads_the_header_after_a_preamble_and_guesses_the_columns() {
        let draft = draft_from_sample(
            b"Statement\nAccount 1234\n\nPost Date,Transaction Date,Description,Debit,Credit,Running Bal.\n08/02/2026,08/01/2026,COFFEE,4.50,,100.00\n",
        )
        .unwrap();
        assert_eq!(draft.skip_rows, 2);
        assert_eq!(draft.spec.date.column, "Post Date");
        assert_eq!(draft.spec.date.format, "%m/%d/%Y");
        assert_eq!(
            draft
                .spec
                .effective_date
                .as_ref()
                .map(|d| d.column.as_str()),
            Some("Transaction Date")
        );
        assert_eq!(
            draft.spec.amount,
            AmountSpec::DebitCredit {
                debit: "Debit".into(),
                credit: "Credit".into()
            }
        );
        assert_eq!(
            draft.spec.payee,
            PayeeSpec::Column {
                column: "Description".into()
            }
        );
        assert_eq!(
            draft.spec.balance.as_ref().map(|c| c.column.as_str()),
            Some("Running Bal.")
        );
        validate_spec(&draft.spec).unwrap();
    }

    #[test]
    fn the_ofx_spec_rejects_unknown_flags() {
        assert!(OfxSpec::parse(r#"{"flags_by_trntype":{"ATM":["cash_withdrawal"]}}"#).is_ok());
        assert!(OfxSpec::parse(r#"{"flags_by_trntype":{"ATM":["nope"]}}"#).is_err());
        assert_eq!(
            OfxSpec::parse("{}").unwrap(),
            OfxSpec {
                flags_by_trntype: BTreeMap::new()
            }
        );
    }
}
