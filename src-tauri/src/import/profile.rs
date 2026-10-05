//! Institution import profiles (ARCHITECTURE §6.1): how one CSV layout maps onto ledger rows.
//! Profiles are data (`import_profile.spec_json`), so a new bank is a row, not a release.

use std::collections::BTreeMap;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Profile {
    pub id: i64,
    pub name: String,
    pub institution: String,
    pub format: String,
    pub is_system: bool,
    pub spec: ProfileSpec,
}

pub fn list(conn: &Connection) -> AppResult<Vec<Profile>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, institution, format, is_system, spec_json FROM import_profile ORDER BY name",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, String>(5)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, name, institution, format, is_system, spec_json) = row?;
        out.push(Profile {
            id,
            name,
            institution,
            format,
            is_system: is_system == 1,
            spec: ProfileSpec::parse(&spec_json)?,
        });
    }
    Ok(out)
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Profile> {
    list(conn)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or(AppError::NotFound {
            entity: "import_profile",
            id,
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
            p.spec.header_signature.len() == folded.len()
                && p.spec
                    .header_signature
                    .iter()
                    .map(|h| fold_header(h))
                    .eq(folded.iter().cloned())
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
}
