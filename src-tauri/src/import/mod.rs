//! The import pipeline (ARCHITECTURE §2, §6): hash → detect → parse → normalise → dedup →
//! commit in one transaction → report. Idempotent, never silent, never overwrites a user edit.

pub mod csv;
pub mod dedup;
pub mod normalize;
pub mod ofx;
pub mod profile;
pub mod report;

use std::collections::BTreeMap;

use chrono::Duration;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cash::recon;
use crate::dates::{format_civil, parse_civil, CivilDate};
use crate::db::audit::{self, Action, Actor};
use crate::db::repo::{account, batch, link, txn};
use crate::error::{AppError, AppResult};
use crate::import::csv::{flag_names, FileClosing, ParsedFile, ParsedRow, RowStatus};
use crate::import::dedup::{Candidate, Decision};
use crate::import::normalize::payee_norm;
use crate::import::ofx::OfxInfo;
use crate::import::profile::{Profile, ProfileSpec, Spec};
use crate::import::report::{ImportReport, Quarantined, Skipped, Updated};

#[derive(Debug, Clone)]
pub struct ImportInput {
    pub account_id: i64,
    pub profile_id: Option<i64>,
    pub file_name: String,
    pub bytes: Vec<u8>,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProfileSummary {
    pub id: i64,
    pub name: String,
    pub institution: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Problem {
    pub row: usize,
    pub column: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PreviewRow {
    pub row: usize,
    pub posted_date: String,
    pub effective_date: String,
    pub amount_cents: i64,
    pub payee_raw: String,
    pub payee_norm: String,
    pub memo: String,
    pub status: RowStatus,
    pub external_id: Option<String>,
    pub flags: Vec<&'static str>,
    pub skipped: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Preview {
    pub file_sha256: String,
    /// `csv` or `ofx`, from the bytes themselves.
    pub format: String,
    pub profile: Option<Profile>,
    pub candidates: Vec<ProfileSummary>,
    pub header: Vec<String>,
    pub rows: Vec<PreviewRow>,
    pub total_rows: usize,
    pub blank_rows: usize,
    pub problem: Option<Problem>,
    /// Set when this exact file was already imported for this account and profile.
    pub already_imported_batch: Option<i64>,
    /// The statement closing the file states (CSV running balance or OFX ledger balance).
    pub closing: Option<FileClosing>,
    /// What an OFX/QFX file says about itself.
    pub ofx: Option<OfxInfo>,
}

pub const PREVIEW_ROWS: usize = 20;

fn summaries(profiles: &[Profile]) -> Vec<ProfileSummary> {
    profiles
        .iter()
        .map(|p| ProfileSummary {
            id: p.id,
            name: p.name.clone(),
            institution: p.institution.clone(),
        })
        .collect()
}

/// The format the bytes are in, decided before any profile: `ofx` or `csv`.
pub fn detect_format(bytes: &[u8]) -> &'static str {
    if ofx::is_ofx(bytes) {
        "ofx"
    } else {
        "csv"
    }
}

/// Parse a file with a profile of its own format: CSV rows through the CSV mapping, OFX rows
/// through the fixed OFX mapping (with the file's own account and balance information).
pub fn parse_file(bytes: &[u8], p: &Profile) -> AppResult<(ParsedFile, Option<OfxInfo>)> {
    match (&p.spec, detect_format(bytes)) {
        (Spec::Csv(spec), "csv") => Ok((csv::parse(bytes, spec)?, None)),
        (Spec::Ofx(spec), "ofx") => {
            let (file, info) = ofx::parse(bytes, spec)?;
            Ok((file, Some(info)))
        }
        (Spec::Csv(_), _) => Err(AppError::validation(
            "profile_id",
            format!("{} is a CSV profile but the file is OFX/QFX", p.name),
        )),
        (Spec::Ofx(_), _) => Err(AppError::validation(
            "profile_id",
            format!("{} is the OFX/QFX profile but the file is not OFX", p.name),
        )),
    }
}

/// The profile to use: the one asked for, else the single profile whose signature matches
/// (CSV), else the OFX profile when the bytes are OFX.
fn resolve_profile(
    conn: &Connection,
    input: &ImportInput,
) -> AppResult<(Option<Profile>, Vec<Profile>, Vec<String>)> {
    let profiles = profile::list(conn)?;
    let format = detect_format(&input.bytes);
    if let Some(id) = input.profile_id {
        let p = profiles
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or(AppError::NotFound {
                entity: "import_profile",
                id,
            })?;
        let header = match p.csv() {
            Some(spec) if format == "csv" => {
                csv::read_header(&input.bytes, spec.skip_rows).unwrap_or_default()
            }
            _ => Vec::new(),
        };
        return Ok((Some(p), Vec::new(), header));
    }
    if format == "ofx" {
        let candidates: Vec<Profile> = profiles.into_iter().filter(|p| p.format == "ofx").collect();
        let chosen = candidates
            .iter()
            .find(|p| p.is_system)
            .or(candidates.first())
            .cloned();
        return Ok((chosen, candidates, Vec::new()));
    }
    let mut candidates = Vec::new();
    for p in &profiles {
        let Some(spec) = p.csv() else {
            continue;
        };
        if let Ok(header) = csv::read_header(&input.bytes, spec.skip_rows) {
            if !profile::matching(std::slice::from_ref(p), &header).is_empty() {
                candidates.push(p.clone());
            }
        }
    }
    let header = csv::read_header(&input.bytes, 0).unwrap_or_default();
    let chosen = if candidates.len() == 1 {
        candidates.first().cloned()
    } else {
        None
    };
    Ok((chosen, candidates, header))
}

fn preview_row(r: &ParsedRow) -> PreviewRow {
    PreviewRow {
        row: r.row,
        posted_date: format_civil(r.posted_date),
        effective_date: format_civil(r.effective_date),
        amount_cents: r.amount_cents,
        payee_raw: r.payee_raw.clone(),
        payee_norm: payee_norm(&r.payee_raw),
        memo: r.memo.clone(),
        status: r.status,
        external_id: r.external_id.clone(),
        flags: flag_names(r.flags),
        skipped: r.skipped.clone(),
    }
}

/// Mapping preview: the detected profile, the first rows as the ledger would see them, and the
/// first problem if the file does not parse.
pub fn preview(conn: &Connection, input: &ImportInput) -> AppResult<Preview> {
    let (chosen, candidates, header) = resolve_profile(conn, input)?;
    let file_sha256 = sha256_hex(&input.bytes);
    let mut preview = Preview {
        file_sha256: file_sha256.clone(),
        format: detect_format(&input.bytes).to_string(),
        profile: chosen.clone(),
        candidates: summaries(&candidates),
        header,
        rows: Vec::new(),
        total_rows: 0,
        blank_rows: 0,
        problem: None,
        already_imported_batch: None,
        closing: None,
        ofx: None,
    };
    let Some(p) = chosen else {
        return Ok(preview);
    };
    preview.already_imported_batch =
        batch::find_same_file(conn, input.account_id, p.id, &file_sha256)?;
    match parse_file(&input.bytes, &p) {
        Ok((file, info)) => {
            if info.is_some() {
                preview.header = file.header.clone();
            }
            fill_preview(&mut preview, &file);
            preview.ofx = info;
        }
        Err(e) => preview.problem = Some(problem_of(e)),
    }
    Ok(preview)
}

fn problem_of(e: AppError) -> Problem {
    match e {
        AppError::Parse {
            row,
            column,
            message,
        } => Problem {
            row,
            column,
            message,
        },
        other => Problem {
            row: 0,
            column: String::new(),
            message: other.to_string(),
        },
    }
}

fn fill_preview(preview: &mut Preview, file: &ParsedFile) {
    preview.total_rows = file.rows.len();
    preview.blank_rows = file.blank_rows;
    preview.closing = file.closing;
    preview.rows = file
        .rows
        .iter()
        .take(PREVIEW_ROWS)
        .map(preview_row)
        .collect();
}

/// Try a CSV mapping against a file without touching the database: what the profile editor's
/// "test against a file" shows (ADR-0046). A preview with no profile and no batch lookup.
pub fn test_spec(bytes: &[u8], spec: &ProfileSpec) -> AppResult<Preview> {
    profile::validate_spec(spec)?;
    let mut preview = Preview {
        file_sha256: sha256_hex(bytes),
        format: detect_format(bytes).to_string(),
        profile: None,
        candidates: Vec::new(),
        header: csv::read_header(bytes, spec.skip_rows).unwrap_or_default(),
        rows: Vec::new(),
        total_rows: 0,
        blank_rows: 0,
        problem: None,
        already_imported_batch: None,
        closing: None,
        ofx: None,
    };
    if preview.format != "csv" {
        preview.problem = Some(Problem {
            row: 0,
            column: String::new(),
            message: "the file is OFX/QFX; a CSV mapping does not apply".into(),
        });
        return Ok(preview);
    }
    if profile::matching(
        &[Profile {
            id: 0,
            name: String::new(),
            institution: String::new(),
            format: "csv".into(),
            is_system: false,
            spec: Spec::Csv(Box::new(spec.clone())),
        }],
        &preview.header,
    )
    .is_empty()
    {
        preview.problem = Some(Problem {
            row: 0,
            column: String::new(),
            message: format!(
                "the file's header ({}) does not match the signature ({})",
                preview.header.join(" | "),
                spec.header_signature.join(" | ")
            ),
        });
        return Ok(preview);
    }
    match csv::parse(bytes, spec) {
        Ok(file) => fill_preview(&mut preview, &file),
        Err(e) => preview.problem = Some(problem_of(e)),
    }
    Ok(preview)
}

fn fuzzy_candidates(
    conn: &Connection,
    account_id: i64,
    amount_cents: i64,
    posted: CivilDate,
) -> AppResult<Vec<Candidate>> {
    let from = format_civil(posted - Duration::days(dedup::FUZZY_WINDOW_DAYS));
    let to = format_civil(posted + Duration::days(dedup::FUZZY_WINDOW_DAYS));
    let mut stmt = conn.prepare_cached(
        "SELECT id, status, payee_norm, external_id, posted_date FROM txn
         WHERE account_id = ?1 AND amount_cents = ?2 AND parent_id IS NULL AND posted_date BETWEEN ?3 AND ?4
         ORDER BY id",
    )?;
    let rows = stmt.query_map(params![account_id, amount_cents, from, to], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (txn_id, status, payee_norm, external_id, posted_date) = row?;
        out.push(Candidate {
            txn_id,
            status: if status == "pending" {
                RowStatus::Pending
            } else {
                RowStatus::Posted
            },
            payee_norm,
            external_id,
            posted_date: parse_civil(&posted_date)?,
        });
    }
    Ok(out)
}

fn exact_match(
    conn: &Connection,
    account_id: i64,
    hash: &str,
    external_id: Option<&str>,
) -> AppResult<Option<(i64, &'static str)>> {
    if let Some(ext) = external_id {
        let hit: Option<i64> = conn
            .prepare_cached("SELECT id FROM txn WHERE account_id = ?1 AND external_id = ?2")?
            .query_row(params![account_id, ext], |r| r.get(0))
            .optional()?;
        if let Some(id) = hit {
            return Ok(Some((id, "external_id")));
        }
    }
    let hit: Option<i64> = conn
        .prepare_cached("SELECT id FROM txn WHERE account_id = ?1 AND source_row_hash = ?2")?
        .query_row(params![account_id, hash], |r| r.get(0))
        .optional()?;
    Ok(hit.map(|id| (id, "hash")))
}

/// Import a file. Everything happens in one transaction; any error leaves nothing written.
pub fn commit(
    conn: &mut Connection,
    input: &ImportInput,
    today: CivilDate,
    threshold_bps: i64,
) -> AppResult<ImportReport> {
    let acct = account::get(conn, input.account_id)?;
    if acct.archived {
        return Err(AppError::validation(
            "account_id",
            format!("{} is archived", acct.name),
        ));
    }
    let (chosen, candidates, _) = resolve_profile(conn, input)?;
    let p = chosen.ok_or_else(|| {
        if candidates.is_empty() {
            AppError::validation(
                "profile_id",
                "no profile matches this file's header; choose one",
            )
        } else {
            AppError::validation(
                "profile_id",
                format!(
                    "{} profiles match this header ({}); choose one",
                    candidates.len(),
                    candidates
                        .iter()
                        .map(|c| c.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        }
    })?;
    let file_sha256 = sha256_hex(&input.bytes);
    let (file, _) = parse_file(&input.bytes, &p)?;

    let opening = parse_civil(&acct.opening_date)?;
    for r in file.rows.iter().filter(|r| r.skipped.is_none()) {
        if r.status == RowStatus::Posted && r.posted_date > today {
            return Err(AppError::validation(
                "posted_date",
                format!(
                    "row {}: posted date {} is in the future",
                    r.row,
                    format_civil(r.posted_date)
                ),
            ));
        }
        if r.posted_date < opening {
            return Err(AppError::validation(
                "posted_date",
                format!(
                    "row {}: dated {}, before the account's opening date {}",
                    r.row,
                    format_civil(r.posted_date),
                    acct.opening_date
                ),
            ));
        }
    }

    let tx = conn.transaction()?;
    let cmd = audit::begin(&tx, "import.commit", Actor::Import)?;
    let mut report = ImportReport {
        rows_read: file.rows.len(),
        blank_rows: file.blank_rows,
        threshold_bps,
        profile_name: p.name.clone(),
        ..Default::default()
    };
    if let Some(closing) = file.closing {
        report.file_closing_cents = Some(closing.cents);
        report.file_closing_date = Some(format_civil(closing.date));
    }

    if let Some(prev) = batch::find_same_file(&tx, acct.id, p.id, &file_sha256)? {
        let batch_id = batch::insert(
            &tx,
            cmd.id,
            &file_sha256,
            &input.file_name,
            acct.id,
            p.id,
            file.rows.len(),
        )?;
        report.batch_id = batch_id;
        report.reason = Some(format!("duplicate_file_of_batch_{prev}"));
        report.skipped = file
            .rows
            .iter()
            .map(|r| Skipped {
                row: r.row,
                matched_txn_id: None,
                by: "duplicate_file".into(),
            })
            .collect();
        report.summary = report.summary_text();
        let json = serde_json::to_string(&report)?;
        batch::finish(
            &tx,
            batch_id,
            &batch::BatchTotals {
                skipped: report.skipped.len(),
                ..Default::default()
            },
            &json,
        )?;
        tx.commit()?;
        tracing::info!(
            batch_id,
            previous = prev,
            "import: duplicate file, nothing to do"
        );
        return Ok(report);
    }

    let batch_id = batch::insert(
        &tx,
        cmd.id,
        &file_sha256,
        &input.file_name,
        acct.id,
        p.id,
        file.rows.len(),
    )?;
    report.batch_id = batch_id;
    let mut date_from: Option<CivilDate> = None;
    let mut date_to: Option<CivilDate> = None;

    for r in &file.rows {
        if let Some(reason) = &r.skipped {
            report.skipped.push(Skipped {
                row: r.row,
                matched_txn_id: None,
                by: "profile_rule".into(),
            });
            tracing::debug!(row = r.row, reason = %reason, "import: row skipped by profile rule");
            continue;
        }
        date_from = Some(date_from.map_or(r.posted_date, |d| d.min(r.posted_date)));
        date_to = Some(date_to.map_or(r.posted_date, |d| d.max(r.posted_date)));
        let hash = dedup::source_row_hash(
            acct.id,
            r.posted_date,
            r.amount_cents,
            &r.payee_raw,
            &r.memo,
            r.external_id.as_deref(),
        );
        let exact = exact_match(&tx, acct.id, &hash, r.external_id.as_deref())?;
        let norm = payee_norm(&r.payee_raw);
        let candidates = if exact.is_none() {
            fuzzy_candidates(&tx, acct.id, r.amount_cents, r.posted_date)?
        } else {
            Vec::new()
        };
        let decision = dedup::decide(
            exact,
            r.status,
            &norm,
            r.external_id.as_deref(),
            &candidates,
            threshold_bps,
        );
        let posted = format_civil(r.posted_date);
        let effective = format_civil(r.effective_date);
        match decision {
            Decision::Skip { txn_id, matched_by } => report.skipped.push(Skipped {
                row: r.row,
                matched_txn_id: Some(txn_id),
                by: matched_by.into(),
            }),
            Decision::SkipOlder { txn_id, .. } => report.skipped.push(Skipped {
                row: r.row,
                matched_txn_id: Some(txn_id),
                by: "older_observation".into(),
            }),
            Decision::Insert => {
                let rec = txn::insert_imported(
                    &tx,
                    &cmd,
                    &txn::NewImportedTxn {
                        account_id: acct.id,
                        import_batch_id: batch_id,
                        posted_date: &posted,
                        effective_date: &effective,
                        amount_cents: r.amount_cents,
                        payee_raw: &r.payee_raw,
                        memo: &r.memo,
                        status: r.status,
                        external_id: r.external_id.as_deref(),
                        source_row_hash: &hash,
                        flags: i64::from(r.flags),
                    },
                )?;
                report.inserted.push(rec.id);
            }
            Decision::Update {
                txn_id,
                similarity_bps,
            } => {
                let fields = txn::update_system_fields(
                    &tx,
                    &cmd,
                    txn_id,
                    &txn::SystemUpdate {
                        posted_date: &posted,
                        effective_date: &effective,
                        payee_raw: &r.payee_raw,
                        memo: &r.memo,
                        status: r.status,
                        external_id: r.external_id.as_deref(),
                        source_row_hash: &hash,
                    },
                )?;
                report.updated.push(Updated {
                    txn_id,
                    row: r.row,
                    similarity_bps,
                    fields,
                });
            }
            Decision::Quarantine {
                txn_id,
                similarity_bps,
            } => {
                if batch::quarantine_pending_by_hash(&tx, acct.id, &hash)?.is_some() {
                    report.skipped.push(Skipped {
                        row: r.row,
                        matched_txn_id: Some(txn_id),
                        by: "already_quarantined".into(),
                    });
                } else {
                    let row_json = serde_json::to_string(r)?;
                    let reason = format!(
                        "same account, amount and date window as row {txn_id}; payee similarity {}.{:02}%",
                        similarity_bps / 100,
                        similarity_bps % 100
                    );
                    let qid = batch::quarantine_insert(
                        &tx,
                        &batch::NewQuarantine {
                            batch_id,
                            account_id: acct.id,
                            row_json: &row_json,
                            hash: &hash,
                            suspected_txn_id: txn_id,
                            similarity_bps,
                            reason: &reason,
                        },
                    )?;
                    report.quarantined.push(Quarantined {
                        row: r.row,
                        quarantine_id: qid,
                        suspected_txn_id: txn_id,
                        similarity_bps,
                    });
                }
            }
        }
    }

    if !report.inserted.is_empty() {
        report.automation = Some(crate::rules::automate(&tx, &cmd, Some(&report.inserted))?);
    }
    report.date_from = date_from.map(format_civil);
    report.date_to = date_to.map(format_civil);
    let (inserted, updated, skipped, quarantined) = report.counts();
    report.summary = report.summary_text();
    let json = serde_json::to_string(&report)?;
    batch::finish(
        &tx,
        batch_id,
        &batch::BatchTotals {
            date_from: report.date_from.as_deref(),
            date_to: report.date_to.as_deref(),
            inserted,
            updated,
            skipped,
            quarantined,
        },
        &json,
    )?;
    recon::refresh_all(&tx, &cmd)?;
    crate::plan::match_all(&tx, &cmd, today)?;
    crate::debt::refresh(&tx, &cmd, today)?;
    tx.commit()?;
    tracing::info!(
        batch_id,
        inserted,
        updated,
        skipped,
        quarantined,
        "import committed"
    );
    Ok(report)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UndoReport {
    pub batch_id: i64,
    pub deleted: usize,
    pub restored: usize,
    pub unlinked: usize,
    pub quarantine_discarded: usize,
}

/// What one import command did to one ledger row. Automation can touch a row several times
/// inside the same command, so undo compares against the state the command left and restores
/// the state it found.
struct RowTrail {
    inserted: bool,
    first_before: Option<String>,
    last_after: Option<String>,
}

fn parse_row(json: &str) -> AppResult<txn::TxnRecord> {
    Ok(serde_json::from_str(json)?)
}

/// Reverse a batch: delete what it inserted, restore what it updated, remove the links its
/// automation made, discard its quarantine rows. Refused when any touched row changed since
/// (undo later batches and edits first).
pub fn undo(conn: &mut Connection, batch_id: i64, today: CivilDate) -> AppResult<UndoReport> {
    let b = batch::get(conn, batch_id)?;
    if b.undone_at.is_some() {
        return Err(AppError::Conflict(format!(
            "batch {batch_id} was already undone"
        )));
    }
    let tx = conn.transaction()?;
    let mut stmt = tx.prepare(
        "SELECT entity_id, action, before_json, after_json FROM audit_event
         WHERE command_id = ?1 AND entity = 'txn' ORDER BY id",
    )?;
    let events = stmt
        .query_map([b.command_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let mut trails: BTreeMap<i64, RowTrail> = BTreeMap::new();
    for (id, action, before_json, after_json) in events {
        let trail = trails.entry(id).or_insert_with(|| RowTrail {
            inserted: false,
            first_before: before_json,
            last_after: None,
        });
        if action == "insert" {
            trail.inserted = true;
        }
        trail.last_after = after_json;
    }

    let mut conflicts = Vec::new();
    for (id, trail) in &trails {
        let Some(expected_json) = &trail.last_after else {
            continue;
        };
        let expected = parse_row(expected_json)?;
        match txn::get(&tx, *id) {
            Ok(current) if current == expected => {}
            Ok(_) => conflicts.push(format!("row {id} changed after the import")),
            Err(AppError::NotFound { .. }) => conflicts.push(format!("row {id} no longer exists")),
            Err(e) => return Err(e),
        }
        if !txn::children(&tx, *id)?.is_empty() {
            conflicts.push(format!("row {id} has been split"));
        }
        if trail.inserted {
            // a refund linked later points at this row without changing it
            for l in link::refunds_of_original(&tx, *id)? {
                if !trails.contains_key(&l.refund_txn_id) {
                    conflicts.push(format!(
                        "row {id} was later linked as the original of a refund"
                    ));
                }
            }
        }
    }
    if !conflicts.is_empty() {
        return Err(AppError::Conflict(format!(
            "cannot undo batch {batch_id}: {}",
            conflicts.join("; ")
        )));
    }

    let undo_cmd = audit::begin(&tx, "import.undo", Actor::Undo)?;
    tx.execute(
        "UPDATE command SET undoes_command_id = ?1 WHERE id = ?2",
        params![b.command_id, undo_cmd.id],
    )?;

    // Links the command made come off first, so restoring one leg never clobbers the other.
    // A link that was already there when the command found the row stays.
    let mut unlinked = 0usize;
    for (id, trail) in &trails {
        let current = txn::get(&tx, *id)?;
        let found = match &trail.first_before {
            Some(j) => Some(parse_row(j)?),
            None => None,
        };
        if let Some(link_id) = current.transfer_link_id {
            if found.as_ref().and_then(|f| f.transfer_link_id) != Some(link_id) {
                link::remove_transfer(&tx, &undo_cmd, link_id)?;
                unlinked += 1;
            }
        }
        if let Some(link_id) = current.refund_link_id {
            if found.as_ref().and_then(|f| f.refund_link_id) != Some(link_id) {
                link::remove_refund(&tx, &undo_cmd, link_id)?;
                unlinked += 1;
            }
        }
    }

    let mut deleted = 0usize;
    let mut restored = 0usize;
    for (id, trail) in trails.iter().rev() {
        if trail.inserted {
            txn::delete_row(&tx, &undo_cmd, *id)?;
            deleted += 1;
            continue;
        }
        let Some(before_json) = &trail.first_before else {
            continue;
        };
        let before = parse_row(before_json)?;
        let current = txn::get(&tx, *id)?;
        txn::write_all_columns(&tx, &before)?;
        audit::record(
            &tx,
            &undo_cmd,
            "txn",
            *id,
            Action::Update,
            Some(&serde_json::to_value(&current)?),
            Some(&serde_json::to_value(&before)?),
        )?;
        restored += 1;
    }
    let mut quarantine_discarded = 0usize;
    for q in batch::quarantine_for_batch(&tx, batch_id)? {
        if q.resolution == "pending" {
            batch::quarantine_resolve(&tx, q.id, "discarded", None)?;
            let mut after = q.clone();
            after.resolution = "discarded".into();
            audit::record(
                &tx,
                &undo_cmd,
                "import_quarantine",
                q.id,
                Action::Update,
                Some(&serde_json::to_value(&q)?),
                Some(&serde_json::to_value(&after)?),
            )?;
            quarantine_discarded += 1;
        }
    }
    batch::mark_undone(&tx, batch_id)?;
    tx.execute(
        "UPDATE command SET undone_by_command_id = ?1 WHERE id = ?2",
        params![undo_cmd.id, b.command_id],
    )?;
    recon::refresh_all(&tx, &undo_cmd)?;
    crate::plan::match_all(&tx, &undo_cmd, today)?;
    crate::debt::refresh(&tx, &undo_cmd, today)?;
    tx.commit()?;
    tracing::info!(
        batch_id,
        deleted,
        restored,
        unlinked,
        quarantine_discarded,
        "import batch undone"
    );
    Ok(UndoReport {
        batch_id,
        deleted,
        restored,
        unlinked,
        quarantine_discarded,
    })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuarantineAction {
    /// The row is a real, separate transaction: insert it.
    Insert,
    /// The row is a duplicate: drop it (the ledger row it matched stays).
    Discard,
}

/// Resolve a suspected duplicate. Both outcomes are audited.
pub fn resolve_quarantine(
    conn: &mut Connection,
    id: i64,
    action: QuarantineAction,
    today: CivilDate,
) -> AppResult<Option<txn::TxnRecord>> {
    let q = batch::quarantine_get(conn, id)?;
    if q.resolution != "pending" {
        return Err(AppError::Conflict(format!(
            "quarantine row {id} was already resolved ({})",
            q.resolution
        )));
    }
    let tx = conn.transaction()?;
    let cmd = audit::begin(&tx, "import.quarantine_resolve", Actor::User)?;
    let inserted = match action {
        QuarantineAction::Insert => {
            let r: ParsedRow = serde_json::from_str(&q.row_json)?;
            let posted = format_civil(r.posted_date);
            let effective = format_civil(r.effective_date);
            let rec = txn::insert_imported(
                &tx,
                &cmd,
                &txn::NewImportedTxn {
                    account_id: q.account_id,
                    import_batch_id: q.import_batch_id,
                    posted_date: &posted,
                    effective_date: &effective,
                    amount_cents: r.amount_cents,
                    payee_raw: &r.payee_raw,
                    memo: &r.memo,
                    status: r.status,
                    external_id: r.external_id.as_deref(),
                    source_row_hash: &q.source_row_hash,
                    flags: i64::from(r.flags),
                },
            )?;
            batch::quarantine_resolve(&tx, id, "inserted", Some(rec.id))?;
            Some(rec)
        }
        QuarantineAction::Discard => {
            batch::quarantine_resolve(&tx, id, "discarded", None)?;
            None
        }
    };
    let after = batch::quarantine_get(&tx, id)?;
    audit::record(
        &tx,
        &cmd,
        "import_quarantine",
        id,
        Action::Update,
        Some(&serde_json::to_value(&q)?),
        Some(&serde_json::to_value(&after)?),
    )?;
    recon::refresh_all(&tx, &cmd)?;
    crate::plan::match_all(&tx, &cmd, today)?;
    crate::debt::refresh(&tx, &cmd, today)?;
    tx.commit()?;
    Ok(inserted)
}
