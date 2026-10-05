//! The import report (ARCHITECTURE §6.3): what happened to every row, stored on the batch and
//! rendered as prose for people.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Updated {
    pub txn_id: i64,
    pub row: usize,
    pub similarity_bps: i64,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Skipped {
    pub row: usize,
    pub matched_txn_id: Option<i64>,
    pub by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Quarantined {
    pub row: usize,
    pub quarantine_id: i64,
    pub suspected_txn_id: i64,
    pub similarity_bps: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ImportReport {
    pub batch_id: i64,
    /// Set when the whole file was a repeat of an earlier batch.
    pub reason: Option<String>,
    pub rows_read: usize,
    pub blank_rows: usize,
    pub inserted: Vec<i64>,
    pub updated: Vec<Updated>,
    pub skipped: Vec<Skipped>,
    pub quarantined: Vec<Quarantined>,
    pub threshold_bps: i64,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub profile_name: String,
    /// Prose rendering of the counts, filled in when the batch finishes (`summary_text`).
    #[serde(default)]
    pub summary: String,
    /// What rules, heuristics and link detection did to the inserted rows.
    #[serde(default)]
    pub automation: Option<crate::rules::AutomationReport>,
    /// The last running balance the file carried (profile `balance` column) and its date: the
    /// statement closing a reconciliation can start from.
    #[serde(default)]
    pub file_closing_cents: Option<i64>,
    #[serde(default)]
    pub file_closing_date: Option<String>,
}

impl ImportReport {
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        (
            self.inserted.len(),
            self.updated.len(),
            self.skipped.len(),
            self.quarantined.len(),
        )
    }

    /// Prose for the import screen and the batch history.
    pub fn summary_text(&self) -> String {
        if let Some(reason) = &self.reason {
            return format!(
                "Read {} rows. Nothing to do: {}.",
                self.rows_read,
                reason.replace('_', " ")
            );
        }
        let (inserted, updated, _, quarantined) = self.counts();
        let mut parts = vec![format!("Read {} rows.", self.rows_read)];
        parts.push(format!("Inserted {inserted}."));
        if updated > 0 {
            parts.push(format!(
                "Updated {updated} already-known row{} with a better observation (pending that posted, or a new reference).",
                if updated == 1 { "" } else { "s" }
            ));
        }
        let by_hash = self
            .skipped
            .iter()
            .filter(|s| s.by == "hash" || s.by == "external_id")
            .count();
        let by_rule = self
            .skipped
            .iter()
            .filter(|s| s.by == "profile_rule")
            .count();
        let older = self
            .skipped
            .iter()
            .filter(|s| s.by == "older_observation")
            .count();
        let held = self
            .skipped
            .iter()
            .filter(|s| s.by == "already_quarantined")
            .count();
        if by_hash > 0 {
            parts.push(format!(
                "Skipped {by_hash} already-imported row{}.",
                if by_hash == 1 { "" } else { "s" }
            ));
        }
        if older > 0 {
            parts.push(format!(
                "Skipped {older} pending row{} already posted in the ledger.",
                if older == 1 { "" } else { "s" }
            ));
        }
        if by_rule > 0 {
            parts.push(format!(
                "Skipped {by_rule} row{} by the profile's rule (see the row list).",
                if by_rule == 1 { "" } else { "s" }
            ));
        }
        if held > 0 {
            parts.push(format!(
                "{held} row{} already waiting in review.",
                if held == 1 { " is" } else { "s are" }
            ));
        }
        if quarantined > 0 {
            parts.push(format!(
                "Held {quarantined} suspected duplicate{} for review (similarity ≥ {}.{:02}%).",
                if quarantined == 1 { "" } else { "s" },
                self.threshold_bps / 100,
                self.threshold_bps % 100
            ));
        }
        if self.blank_rows > 0 {
            parts.push(format!(
                "Ignored {} blank line{}.",
                self.blank_rows,
                if self.blank_rows == 1 { "" } else { "s" }
            ));
        }
        if let Some(a) = &self.automation {
            let text = a.summary_text();
            if !text.is_empty() {
                parts.push(text);
            }
        }
        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_reads_as_prose() {
        let mut r = ImportReport {
            rows_read: 32,
            threshold_bps: 8500,
            ..Default::default()
        };
        r.inserted = vec![1];
        r.skipped = (0..31)
            .map(|i| Skipped {
                row: i,
                matched_txn_id: Some(1),
                by: "hash".into(),
            })
            .collect();
        r.quarantined = vec![Quarantined {
            row: 9,
            quarantine_id: 1,
            suspected_txn_id: 20,
            similarity_bps: 9111,
        }];
        assert_eq!(
            r.summary_text(),
            "Read 32 rows. Inserted 1. Skipped 31 already-imported rows. Held 1 suspected duplicate for review (similarity ≥ 85.00%)."
        );
        let no_op = ImportReport {
            rows_read: 14,
            reason: Some("duplicate_file_of_batch_3".into()),
            ..Default::default()
        };
        assert_eq!(
            no_op.summary_text(),
            "Read 14 rows. Nothing to do: duplicate file of batch 3."
        );
    }
}
