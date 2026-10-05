//! Tauri commands: thin wrappers that validate, call the engines, and map errors
//! (ARCHITECTURE §7). Every write runs in one transaction under one command group and emits
//! `kept://changed` so the webview recomputes. Nothing monetary is computed here.

use std::path::PathBuf;

use rusqlite::Transaction;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::cash::recon::{self, DifferenceExplorer, ReconInput, Reconciliation, TrustReport};
use crate::cash::safe::{self, SafeToSpend, Upcoming};
use crate::cash::views::{self, CashView, SpendingView};
use crate::config::{self, DataPaths};
use crate::dates::{parse_civil, parse_zone, today_in, CivilDate};
use crate::db::audit::{self, Actor, CommandRecord};
use crate::db::repo::account::{self, Account, AccountPatch, NewAccount};
use crate::db::repo::batch::{self, ImportBatch, QuarantineRow};
use crate::db::repo::category::{self, Category, NewCategory};
use crate::db::repo::ledger::{self, Cursor, LedgerFilter, LedgerPage, LedgerRow};
use crate::db::repo::link::{self, Confidence, RefundLink, TransferKind, TransferLink};
use crate::db::repo::policy::{self, Policy};
use crate::db::repo::rule::{self, Rule, RuleInput};
use crate::db::repo::saved_view::{self, SavedView};
use crate::db::repo::txn::{self, SplitPart, TxnPatch, TxnRecord};
use crate::db::repo::venture::{self, Venture, VentureInput};
use crate::db::settings::{self, Settings};
use crate::db::{Db, OpenMode};
use crate::error::{AppError, AppResult};
use crate::forecast::{self, variable::CategoryModel, Forecast, PlanOverlay, Scenario};
use crate::import::profile::{self, Profile};
use crate::import::report::ImportReport;
use crate::import::{self, ImportInput, Preview, QuarantineAction, UndoReport};
use crate::plan::earmark::{self, Earmark, EarmarkInput, Entry, EntryInput};
use crate::plan::income::{self, IncomeInput, IncomeStream, Receipt};
use crate::plan::obligation::{self, Obligation, ObligationInput, Payment};
use crate::rules::link::{self as detect, Candidate};
use crate::rules::{self, AutomationReport, RuleProposal};
use crate::{poisoned, secret, AppState};

pub const CHANGED_EVENT: &str = "kept://changed";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppStateKind {
    NeedsDataDir,
    NeedsDatabase,
    Locked,
    Unlocked,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppStatus {
    pub state: AppStateKind,
    pub data_dir: Option<String>,
    pub remembered: bool,
    pub version: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct Changed<'a> {
    entities: &'a [&'a str],
}

fn status(state: &AppState) -> AppResult<AppStatus> {
    let paths = state.paths.lock().map_err(|_| poisoned())?;
    let Some(paths) = paths.as_ref() else {
        return Ok(AppStatus {
            state: AppStateKind::NeedsDataDir,
            data_dir: None,
            remembered: false,
            version: env!("CARGO_PKG_VERSION"),
        });
    };
    let unlocked = state.db.lock().map_err(|_| poisoned())?.is_some();
    let kind = if unlocked {
        AppStateKind::Unlocked
    } else if paths.db_exists() {
        AppStateKind::Locked
    } else {
        AppStateKind::NeedsDatabase
    };
    let remembered = secret::is_remembered(&paths.root).unwrap_or(false);
    Ok(AppStatus {
        state: kind,
        data_dir: Some(paths.root.display().to_string()),
        remembered,
        version: env!("CARGO_PKG_VERSION"),
    })
}

fn current_paths(state: &AppState) -> AppResult<DataPaths> {
    state
        .paths
        .lock()
        .map_err(|_| poisoned())?
        .clone()
        .ok_or_else(|| AppError::validation("data_dir", "choose a data folder first"))
}

fn emit_changed(app: &AppHandle, entities: &[&str]) {
    if let Err(e) = app.emit(CHANGED_EVENT, Changed { entities }) {
        tracing::warn!(error = %e, "could not emit change event");
    }
}

/// Run `f` against the unlocked database.
fn with_db<T>(state: &AppState, f: impl FnOnce(&mut Db) -> AppResult<T>) -> AppResult<T> {
    let mut guard = state.db.lock().map_err(|_| poisoned())?;
    let db = guard.as_mut().ok_or(AppError::Locked)?;
    f(db)
}

/// Run `f` inside one transaction under one user command group; commit on success.
fn write<T>(
    state: &AppState,
    name: &str,
    f: impl FnOnce(&Transaction, &CommandRecord) -> AppResult<T>,
) -> AppResult<T> {
    with_db(state, |db| {
        let today = today(db)?;
        let tx = db.conn_mut().transaction()?;
        let cmd = audit::begin(&tx, name, Actor::User)?;
        let out = f(&tx, &cmd)?;
        recon::refresh_all(&tx, &cmd)?;
        crate::plan::match_all(&tx, &cmd, today)?;
        tx.commit()?;
        Ok(out)
    })
}

fn today(db: &Db) -> AppResult<CivilDate> {
    let zone = parse_zone(&settings::load(db.conn())?.zone)?;
    Ok(today_in(zone))
}

// ---- app lifecycle ------------------------------------------------------------------------

#[tauri::command]
pub async fn app_status(state: State<'_, AppState>) -> AppResult<AppStatus> {
    status(&state)
}

/// Adopt a data folder (first run, or moving to another folder). Writes `kept.config.json`
/// beside the executable unless `KEPT_DATA_DIR` is set.
#[tauri::command]
pub async fn choose_data_dir(state: State<'_, AppState>, path: String) -> AppResult<AppStatus> {
    let dir = PathBuf::from(path.trim());
    if dir.as_os_str().is_empty() || !dir.is_absolute() {
        return Err(AppError::validation(
            "data_dir",
            "the data folder must be an absolute path",
        ));
    }
    if state.db.lock().map_err(|_| poisoned())?.is_some() {
        return Err(AppError::Conflict(
            "lock the current database before changing the data folder".into(),
        ));
    }
    let paths = DataPaths::new(dir);
    state.adopt_paths(paths.clone())?;
    config::save_data_dir(&paths.root)?;
    tracing::info!("data folder chosen");
    status(&state)
}

#[tauri::command]
pub async fn create_database(
    state: State<'_, AppState>,
    passphrase: String,
    confirm: String,
    remember: bool,
) -> AppResult<AppStatus> {
    if passphrase != confirm {
        return Err(AppError::validation(
            "confirm",
            "the two passphrases do not match",
        ));
    }
    let paths = current_paths(&state)?;
    let db = Db::open(&paths, &passphrase, OpenMode::CreateNew)?;
    *state.db.lock().map_err(|_| poisoned())? = Some(db);
    if remember {
        secret::remember(&paths.root, &passphrase)?;
    }
    tracing::info!("database created");
    status(&state)
}

#[tauri::command]
pub async fn unlock(
    state: State<'_, AppState>,
    passphrase: String,
    remember: bool,
) -> AppResult<AppStatus> {
    let paths = current_paths(&state)?;
    let db = Db::open(&paths, &passphrase, OpenMode::Existing)?;
    *state.db.lock().map_err(|_| poisoned())? = Some(db);
    if remember {
        secret::remember(&paths.root, &passphrase)?;
    }
    tracing::info!("database unlocked");
    status(&state)
}

#[tauri::command]
pub async fn unlock_remembered(state: State<'_, AppState>) -> AppResult<AppStatus> {
    let paths = current_paths(&state)?;
    let Some(passphrase) = secret::load(&paths.root)? else {
        return Err(AppError::validation(
            "passphrase",
            "no passphrase is remembered for this data folder",
        ));
    };
    let db = Db::open(&paths, &passphrase, OpenMode::Existing)?;
    *state.db.lock().map_err(|_| poisoned())? = Some(db);
    tracing::info!("database unlocked from credential store");
    status(&state)
}

#[tauri::command]
pub async fn lock(state: State<'_, AppState>) -> AppResult<AppStatus> {
    let previous = state.db.lock().map_err(|_| poisoned())?.take();
    drop(previous);
    tracing::info!("database locked");
    status(&state)
}

#[tauri::command]
pub async fn remember_passphrase(state: State<'_, AppState>) -> AppResult<AppStatus> {
    let paths = current_paths(&state)?;
    with_db(&state, |db| secret::remember(&paths.root, db.passphrase()))?;
    status(&state)
}

#[tauri::command]
pub async fn forget_remembered(state: State<'_, AppState>) -> AppResult<AppStatus> {
    let paths = current_paths(&state)?;
    secret::forget(&paths.root)?;
    status(&state)
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> AppResult<Settings> {
    with_db(&state, |db| settings::load(db.conn()))
}

#[tauri::command]
pub async fn update_setting(
    app: AppHandle,
    state: State<'_, AppState>,
    key: String,
    value: serde_json::Value,
) -> AppResult<Settings> {
    let updated = with_db(&state, |db| settings::update(db.conn_mut(), &key, &value))?;
    emit_changed(&app, &["setting"]);
    Ok(updated)
}

// ---- accounts and categories ------------------------------------------------------------------

#[tauri::command]
pub async fn list_accounts(state: State<'_, AppState>) -> AppResult<Vec<Account>> {
    with_db(&state, |db| account::list(db.conn()))
}

#[tauri::command]
pub async fn create_account(
    app: AppHandle,
    state: State<'_, AppState>,
    new: NewAccount,
) -> AppResult<Account> {
    let created = write(&state, "account.create", |tx, cmd| {
        account::create(tx, cmd, &new)
    })?;
    emit_changed(&app, &["account"]);
    Ok(created)
}

#[tauri::command]
pub async fn update_account(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    patch: AccountPatch,
) -> AppResult<Account> {
    let updated = write(&state, "account.update", |tx, cmd| {
        account::update(tx, cmd, id, &patch)
    })?;
    emit_changed(&app, &["account", "txn"]);
    Ok(updated)
}

#[tauri::command]
pub async fn list_categories(state: State<'_, AppState>) -> AppResult<Vec<Category>> {
    with_db(&state, |db| category::list(db.conn()))
}

#[tauri::command]
pub async fn create_category(
    app: AppHandle,
    state: State<'_, AppState>,
    new: NewCategory,
) -> AppResult<Category> {
    let created = write(&state, "category.create", |tx, cmd| {
        category::create(tx, cmd, &new)
    })?;
    emit_changed(&app, &["category"]);
    Ok(created)
}

#[tauri::command]
pub async fn rename_category(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    name: String,
) -> AppResult<Category> {
    let updated = write(&state, "category.rename", |tx, cmd| {
        category::rename(tx, cmd, id, &name)
    })?;
    emit_changed(&app, &["category", "txn"]);
    Ok(updated)
}

#[tauri::command]
pub async fn archive_category(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    archived: bool,
) -> AppResult<Category> {
    let updated = write(&state, "category.archive", |tx, cmd| {
        category::archive(tx, cmd, id, archived)
    })?;
    emit_changed(&app, &["category"]);
    Ok(updated)
}

// ---- import -------------------------------------------------------------------------------------

/// Where the statement comes from: a file the person picked or dropped, or pasted text.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportSource {
    Path { path: String },
    Text { name: String, text: String },
}

fn load_source(source: &ImportSource) -> AppResult<(String, Vec<u8>)> {
    match source {
        ImportSource::Path { path } => {
            let p = PathBuf::from(path);
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone());
            let bytes = std::fs::read(&p).map_err(|e| AppError::io(&p, e))?;
            Ok((name, bytes))
        }
        ImportSource::Text { name, text } => {
            let name = if name.trim().is_empty() {
                "pasted.csv".to_string()
            } else {
                name.trim().to_string()
            };
            Ok((name, text.as_bytes().to_vec()))
        }
    }
}

#[tauri::command]
pub async fn list_import_profiles(state: State<'_, AppState>) -> AppResult<Vec<Profile>> {
    with_db(&state, |db| profile::list(db.conn()))
}

#[tauri::command]
pub async fn import_preview(
    state: State<'_, AppState>,
    account_id: i64,
    profile_id: Option<i64>,
    source: ImportSource,
) -> AppResult<Preview> {
    let (file_name, bytes) = load_source(&source)?;
    with_db(&state, |db| {
        import::preview(
            db.conn(),
            &ImportInput {
                account_id,
                profile_id,
                file_name,
                bytes,
            },
        )
    })
}

#[tauri::command]
pub async fn import_commit(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: i64,
    profile_id: Option<i64>,
    source: ImportSource,
) -> AppResult<ImportReport> {
    let (file_name, bytes) = load_source(&source)?;
    let report = with_db(&state, |db| {
        let today = today(db)?;
        let threshold = settings::load(db.conn())?.dedup_similarity_bps;
        import::commit(
            db.conn_mut(),
            &ImportInput {
                account_id,
                profile_id,
                file_name,
                bytes,
            },
            today,
            threshold,
        )
    })?;
    emit_changed(&app, &["txn", "import_batch", "import_quarantine"]);
    Ok(report)
}

#[tauri::command]
pub async fn undo_import_batch(
    app: AppHandle,
    state: State<'_, AppState>,
    batch_id: i64,
) -> AppResult<UndoReport> {
    let report = with_db(&state, |db| import::undo(db.conn_mut(), batch_id))?;
    emit_changed(&app, &["txn", "import_batch", "import_quarantine"]);
    Ok(report)
}

#[tauri::command]
pub async fn list_import_batches(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> AppResult<Vec<ImportBatch>> {
    with_db(&state, |db| batch::list(db.conn(), limit.unwrap_or(50)))
}

#[tauri::command]
pub async fn list_quarantine(state: State<'_, AppState>) -> AppResult<Vec<QuarantineRow>> {
    with_db(&state, |db| batch::quarantine_pending(db.conn()))
}

#[tauri::command]
pub async fn resolve_quarantine(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    action: QuarantineAction,
) -> AppResult<Option<TxnRecord>> {
    let inserted = with_db(&state, |db| {
        let today = today(db)?;
        import::resolve_quarantine(db.conn_mut(), id, action, today)
    })?;
    emit_changed(&app, &["txn", "import_quarantine"]);
    Ok(inserted)
}

// ---- ledger -------------------------------------------------------------------------------------

#[tauri::command]
pub async fn ledger_query(
    state: State<'_, AppState>,
    filter: LedgerFilter,
    cursor: Option<Cursor>,
    limit: Option<usize>,
) -> AppResult<LedgerPage> {
    with_db(&state, |db| {
        ledger::query(db.conn(), &filter, cursor.as_ref(), limit.unwrap_or(200))
    })
}

#[tauri::command]
pub async fn ledger_children(
    state: State<'_, AppState>,
    parent_id: i64,
) -> AppResult<Vec<TxnRecord>> {
    with_db(&state, |db| txn::children(db.conn(), parent_id))
}

#[tauri::command]
pub async fn get_txn(state: State<'_, AppState>, id: i64) -> AppResult<TxnRecord> {
    with_db(&state, |db| txn::get(db.conn(), id))
}

#[tauri::command]
pub async fn update_txn(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    patch: TxnPatch,
) -> AppResult<TxnRecord> {
    let updated = write(&state, "txn.update", |tx, cmd| {
        txn::apply_user_patch(tx, cmd, id, &patch)
    })?;
    emit_changed(&app, &["txn"]);
    Ok(updated)
}

#[tauri::command]
pub async fn recategorize(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<i64>,
    category_id: Option<i64>,
) -> AppResult<usize> {
    if ids.is_empty() {
        return Ok(0);
    }
    let n = write(&state, "txn.recategorize", |tx, cmd| {
        let patch = TxnPatch {
            category_id: Some(category_id),
            ..Default::default()
        };
        for id in &ids {
            txn::apply_user_patch(tx, cmd, *id, &patch)?;
        }
        Ok(ids.len())
    })?;
    emit_changed(&app, &["txn"]);
    Ok(n)
}

#[tauri::command]
pub async fn split_txn(
    app: AppHandle,
    state: State<'_, AppState>,
    parent_id: i64,
    parts: Vec<SplitPart>,
) -> AppResult<Vec<TxnRecord>> {
    let children = write(&state, "txn.split", |tx, cmd| {
        txn::split(tx, cmd, parent_id, &parts)
    })?;
    emit_changed(&app, &["txn"]);
    Ok(children)
}

#[tauri::command]
pub async fn unsplit_txn(
    app: AppHandle,
    state: State<'_, AppState>,
    parent_id: i64,
) -> AppResult<usize> {
    let n = write(&state, "txn.unsplit", |tx, cmd| {
        txn::unsplit(tx, cmd, parent_id)
    })?;
    emit_changed(&app, &["txn"]);
    Ok(n)
}

#[tauri::command]
pub async fn list_saved_views(state: State<'_, AppState>) -> AppResult<Vec<SavedView>> {
    with_db(&state, |db| saved_view::list(db.conn()))
}

#[tauri::command]
pub async fn save_view(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    query_text: String,
) -> AppResult<SavedView> {
    let view = write(&state, "saved_view.save", |tx, cmd| {
        saved_view::save(tx, cmd, &name, &query_text)
    })?;
    emit_changed(&app, &["saved_view"]);
    Ok(view)
}

#[tauri::command]
pub async fn delete_saved_view(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> AppResult<()> {
    write(&state, "saved_view.delete", |tx, cmd| {
        saved_view::delete(tx, cmd, id)
    })?;
    emit_changed(&app, &["saved_view"]);
    Ok(())
}

// ---- rules --------------------------------------------------------------------------------------

#[tauri::command]
pub async fn list_rules(state: State<'_, AppState>) -> AppResult<Vec<Rule>> {
    with_db(&state, |db| rule::list(db.conn()))
}

#[tauri::command]
pub async fn create_rule(
    app: AppHandle,
    state: State<'_, AppState>,
    input: RuleInput,
) -> AppResult<Rule> {
    let created = write(&state, "rule.create", |tx, cmd| {
        rule::create(tx, cmd, &input)
    })?;
    emit_changed(&app, &["rule"]);
    Ok(created)
}

#[tauri::command]
pub async fn update_rule(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    input: RuleInput,
) -> AppResult<Rule> {
    let updated = write(&state, "rule.update", |tx, cmd| {
        rule::update(tx, cmd, id, &input)
    })?;
    emit_changed(&app, &["rule"]);
    Ok(updated)
}

#[tauri::command]
pub async fn delete_rule(app: AppHandle, state: State<'_, AppState>, id: i64) -> AppResult<()> {
    write(&state, "rule.delete", |tx, cmd| rule::delete(tx, cmd, id))?;
    emit_changed(&app, &["rule"]);
    Ok(())
}

#[tauri::command]
pub async fn reorder_rules(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<i64>,
) -> AppResult<Vec<Rule>> {
    let rules = write(&state, "rule.reorder", |tx, cmd| {
        rule::reorder(tx, cmd, &ids)
    })?;
    emit_changed(&app, &["rule"]);
    Ok(rules)
}

/// Run rules, heuristics and link detection over every row automation may still touch.
#[tauri::command]
pub async fn apply_rules(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<AutomationReport> {
    let report = write(&state, "rules.apply", |tx, cmd| {
        rules::automate(tx, cmd, None)
    })?;
    emit_changed(&app, &["txn", "rule", "transfer_link", "refund_link"]);
    Ok(report)
}

/// The rule a correction suggests. Nothing is created until `create_rule` is called.
#[tauri::command]
pub async fn propose_rule(state: State<'_, AppState>, txn_id: i64) -> AppResult<RuleProposal> {
    with_db(&state, |db| rules::propose_rule(db.conn(), txn_id))
}

// ---- links --------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct RefundCandidate {
    pub candidate: Candidate,
    pub similarity_bps: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LinkCandidates {
    pub transfers: Vec<Candidate>,
    pub refunds: Vec<RefundCandidate>,
}

/// Rows `txn_id` could be linked to: the opposite leg of a transfer on another account, or
/// (for an inflow) an earlier purchase of the same size on the same account, any payee.
#[tauri::command]
pub async fn link_candidates(state: State<'_, AppState>, txn_id: i64) -> AppResult<LinkCandidates> {
    with_db(&state, |db| {
        let row = txn::get(db.conn(), txn_id)?;
        let transfers = detect::transfer_candidates(db.conn(), &row)?;
        let refunds = if row.amount_cents > 0 {
            detect::refund_candidates(db.conn(), &row, 0)?
                .into_iter()
                .map(|(candidate, similarity_bps)| RefundCandidate {
                    candidate,
                    similarity_bps,
                })
                .collect()
        } else {
            Vec::new()
        };
        Ok(LinkCandidates { transfers, refunds })
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct LinkDetails {
    pub transfer: Option<TransferLink>,
    pub transfer_other: Option<TxnRecord>,
    pub refund: Option<RefundLink>,
    pub refund_other: Option<TxnRecord>,
}

/// The links a row is part of, with the row on the other side of each.
#[tauri::command]
pub async fn link_details(state: State<'_, AppState>, txn_id: i64) -> AppResult<LinkDetails> {
    with_db(&state, |db| {
        let conn = db.conn();
        let row = txn::get(conn, txn_id)?;
        let (transfer, transfer_other) = match row.transfer_link_id {
            Some(id) => {
                let l = link::get_transfer(conn, id)?;
                let other = if l.out_txn_id == txn_id {
                    l.in_txn_id
                } else {
                    l.out_txn_id
                };
                (Some(l), Some(txn::get(conn, other)?))
            }
            None => (None, None),
        };
        let (refund, refund_other) = match row.refund_link_id {
            Some(id) => {
                let l = link::get_refund(conn, id)?;
                let other = txn::get(conn, l.original_txn_id)?;
                (Some(l), Some(other))
            }
            None => (None, None),
        };
        Ok(LinkDetails {
            transfer,
            transfer_other,
            refund,
            refund_other,
        })
    })
}

/// Link two rows as a transfer by hand. Without a kind, the accounts decide it.
#[tauri::command]
pub async fn link_transfer(
    app: AppHandle,
    state: State<'_, AppState>,
    out_txn_id: i64,
    in_txn_id: i64,
    kind: Option<String>,
) -> AppResult<TransferLink> {
    let created = write(&state, "link.transfer", |tx, cmd| {
        let kind = match kind {
            Some(k) => TransferKind::parse(&k)?,
            None => {
                let out = txn::get(tx, out_txn_id)?;
                let into = txn::get(tx, in_txn_id)?;
                TransferKind::infer(
                    &account::get(tx, out.account_id)?,
                    &account::get(tx, into.account_id)?,
                )
            }
        };
        link::create_transfer(tx, cmd, out_txn_id, in_txn_id, kind, Confidence::User)
    })?;
    emit_changed(&app, &["txn", "transfer_link"]);
    Ok(created)
}

#[tauri::command]
pub async fn unlink_transfer(
    app: AppHandle,
    state: State<'_, AppState>,
    link_id: i64,
) -> AppResult<()> {
    write(&state, "link.unlink_transfer", |tx, cmd| {
        link::remove_transfer(tx, cmd, link_id)
    })?;
    emit_changed(&app, &["txn", "transfer_link"]);
    Ok(())
}

#[tauri::command]
pub async fn link_refund(
    app: AppHandle,
    state: State<'_, AppState>,
    original_txn_id: i64,
    refund_txn_id: i64,
) -> AppResult<RefundLink> {
    let created = write(&state, "link.refund", |tx, cmd| {
        link::create_refund(tx, cmd, original_txn_id, refund_txn_id, Confidence::User)
    })?;
    emit_changed(&app, &["txn", "refund_link"]);
    Ok(created)
}

#[tauri::command]
pub async fn unlink_refund(
    app: AppHandle,
    state: State<'_, AppState>,
    link_id: i64,
) -> AppResult<()> {
    write(&state, "link.unlink_refund", |tx, cmd| {
        link::remove_refund(tx, cmd, link_id)
    })?;
    emit_changed(&app, &["txn", "refund_link"]);
    Ok(())
}

/// Record the in-app acknowledgment of an outflow from a firewalled account (policy 1).
#[tauri::command]
pub async fn acknowledge_firewall(
    app: AppHandle,
    state: State<'_, AppState>,
    txn_id: i64,
    note: String,
) -> AppResult<TxnRecord> {
    let row = write(&state, "firewall.acknowledge", |tx, cmd| {
        link::acknowledge_firewall(tx, cmd, txn_id, &note)
    })?;
    emit_changed(&app, &["txn", "firewall_ack"]);
    Ok(row)
}

// ---- review queue and views ---------------------------------------------------------------------

#[tauri::command]
pub async fn review_queue(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> AppResult<Vec<LedgerRow>> {
    with_db(&state, |db| {
        ledger::review_queue(db.conn(), limit.unwrap_or(500))
    })
}

fn date_range(from: &str, to: &str) -> AppResult<()> {
    let f = parse_civil(from)?;
    let t = parse_civil(to)?;
    if f > t {
        return Err(AppError::validation(
            "to",
            "the range ends before it starts",
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn spending_view(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> AppResult<SpendingView> {
    date_range(&from, &to)?;
    with_db(&state, |db| views::spending_view(db.conn(), &from, &to))
}

#[tauri::command]
pub async fn cash_view(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> AppResult<CashView> {
    date_range(&from, &to)?;
    with_db(&state, |db| views::cash_view(db.conn(), &from, &to))
}

// ---- ventures -----------------------------------------------------------------------------------

#[tauri::command]
pub async fn list_ventures(state: State<'_, AppState>) -> AppResult<Vec<Venture>> {
    with_db(&state, |db| venture::list(db.conn()))
}

#[tauri::command]
pub async fn create_venture(
    app: AppHandle,
    state: State<'_, AppState>,
    input: VentureInput,
) -> AppResult<Venture> {
    let created = write(&state, "venture.create", |tx, cmd| {
        venture::create(tx, cmd, &input)
    })?;
    emit_changed(&app, &["venture"]);
    Ok(created)
}

#[tauri::command]
pub async fn update_venture(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    input: VentureInput,
) -> AppResult<Venture> {
    let updated = write(&state, "venture.update", |tx, cmd| {
        venture::update(tx, cmd, id, &input)
    })?;
    emit_changed(&app, &["venture", "account"]);
    Ok(updated)
}

// ---- reconciliation and trust -------------------------------------------------------------------

#[tauri::command]
pub async fn list_reconciliations(
    state: State<'_, AppState>,
    account_id: i64,
) -> AppResult<Vec<Reconciliation>> {
    with_db(&state, |db| recon::list(db.conn(), account_id))
}

/// Enter (or re-enter an off) statement balance; every period of the account recomputes.
#[tauri::command]
pub async fn reconcile(
    app: AppHandle,
    state: State<'_, AppState>,
    input: ReconInput,
) -> AppResult<Reconciliation> {
    let r = write(&state, "recon.reconcile", |tx, cmd| {
        recon::reconcile(tx, cmd, &input)
    })?;
    emit_changed(&app, &["reconciliation"]);
    Ok(r)
}

#[tauri::command]
pub async fn delete_reconciliation(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> AppResult<()> {
    write(&state, "recon.delete", |tx, cmd| recon::delete(tx, cmd, id))?;
    emit_changed(&app, &["reconciliation"]);
    Ok(())
}

#[tauri::command]
pub async fn difference_explorer(
    state: State<'_, AppState>,
    id: i64,
) -> AppResult<DifferenceExplorer> {
    with_db(&state, |db| recon::explorer(db.conn(), id))
}

/// Trust per account and for the hero, as of today in the configured zone.
#[tauri::command]
pub async fn trust_status(state: State<'_, AppState>) -> AppResult<TrustReport> {
    with_db(&state, |db| {
        let today = today(db)?;
        let stale = settings::load(db.conn())?.recon_stale_after_days;
        recon::trust(db.conn(), today, stale)
    })
}

// ---- plan: income streams, obligations, earmarks ------------------------------------------------

#[tauri::command]
pub async fn list_income_streams(state: State<'_, AppState>) -> AppResult<Vec<IncomeStream>> {
    with_db(&state, |db| income::list(db.conn()))
}

#[tauri::command]
pub async fn create_income_stream(
    app: AppHandle,
    state: State<'_, AppState>,
    input: IncomeInput,
) -> AppResult<IncomeStream> {
    let created = write(&state, "income_stream.create", |tx, cmd| {
        income::create(tx, cmd, &input)
    })?;
    emit_changed(&app, &["income_stream"]);
    Ok(created)
}

#[tauri::command]
pub async fn update_income_stream(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    input: IncomeInput,
) -> AppResult<IncomeStream> {
    let updated = write(&state, "income_stream.update", |tx, cmd| {
        income::update(tx, cmd, id, &input)
    })?;
    emit_changed(&app, &["income_stream"]);
    Ok(updated)
}

#[tauri::command]
pub async fn list_receipts(state: State<'_, AppState>, stream_id: i64) -> AppResult<Vec<Receipt>> {
    with_db(&state, |db| income::receipts(db.conn(), stream_id))
}

#[tauri::command]
pub async fn record_receipt(
    app: AppHandle,
    state: State<'_, AppState>,
    stream_id: i64,
    due_date: String,
    txn_id: i64,
) -> AppResult<Receipt> {
    let receipt = write(&state, "income_receipt.record", |tx, cmd| {
        income::record_receipt(tx, cmd, stream_id, &due_date, txn_id, "user")
    })?;
    emit_changed(&app, &["income_stream"]);
    Ok(receipt)
}

#[tauri::command]
pub async fn remove_receipt(
    app: AppHandle,
    state: State<'_, AppState>,
    stream_id: i64,
    due_date: String,
) -> AppResult<()> {
    write(&state, "income_receipt.remove", |tx, cmd| {
        income::remove_receipt(tx, cmd, stream_id, &due_date)
    })?;
    emit_changed(&app, &["income_stream"]);
    Ok(())
}

#[tauri::command]
pub async fn list_obligations(state: State<'_, AppState>) -> AppResult<Vec<Obligation>> {
    with_db(&state, |db| obligation::list(db.conn()))
}

#[tauri::command]
pub async fn create_obligation(
    app: AppHandle,
    state: State<'_, AppState>,
    input: ObligationInput,
) -> AppResult<Obligation> {
    let created = write(&state, "obligation.create", |tx, cmd| {
        obligation::create(tx, cmd, &input)
    })?;
    emit_changed(&app, &["obligation"]);
    Ok(created)
}

#[tauri::command]
pub async fn update_obligation(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    input: ObligationInput,
) -> AppResult<Obligation> {
    let updated = write(&state, "obligation.update", |tx, cmd| {
        obligation::update(tx, cmd, id, &input)
    })?;
    emit_changed(&app, &["obligation"]);
    Ok(updated)
}

/// Confirm a candidate, retire an obligation, or bring one back.
#[tauri::command]
pub async fn set_obligation_status(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    status: String,
) -> AppResult<Obligation> {
    let updated = write(&state, "obligation.status", |tx, cmd| {
        obligation::set_status(tx, cmd, id, &status)
    })?;
    emit_changed(&app, &["obligation"]);
    Ok(updated)
}

#[tauri::command]
pub async fn delete_obligation_candidate(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> AppResult<()> {
    write(&state, "obligation.delete_candidate", |tx, cmd| {
        obligation::delete_candidate(tx, cmd, id)
    })?;
    emit_changed(&app, &["obligation"]);
    Ok(())
}

/// Propose candidates from recurring rows; each is a row the person confirms or deletes.
#[tauri::command]
pub async fn detect_obligation_candidates(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<Vec<Obligation>> {
    let created = write(&state, "obligation.detect", |tx, cmd| {
        obligation::detect_candidates(tx, cmd)
    })?;
    emit_changed(&app, &["obligation"]);
    Ok(created)
}

#[tauri::command]
pub async fn list_payments(
    state: State<'_, AppState>,
    obligation_id: i64,
) -> AppResult<Vec<Payment>> {
    with_db(&state, |db| obligation::payments(db.conn(), obligation_id))
}

#[tauri::command]
pub async fn record_payment(
    app: AppHandle,
    state: State<'_, AppState>,
    obligation_id: i64,
    due_date: String,
    txn_id: i64,
) -> AppResult<Payment> {
    let payment = write(&state, "obligation_payment.record", |tx, cmd| {
        obligation::record_payment(tx, cmd, obligation_id, &due_date, txn_id, "user")
    })?;
    emit_changed(&app, &["obligation"]);
    Ok(payment)
}

#[tauri::command]
pub async fn remove_payment(
    app: AppHandle,
    state: State<'_, AppState>,
    obligation_id: i64,
    due_date: String,
) -> AppResult<()> {
    write(&state, "obligation_payment.remove", |tx, cmd| {
        obligation::remove_payment(tx, cmd, obligation_id, &due_date)
    })?;
    emit_changed(&app, &["obligation"]);
    Ok(())
}

#[tauri::command]
pub async fn list_earmarks(state: State<'_, AppState>) -> AppResult<Vec<Earmark>> {
    with_db(&state, |db| earmark::list(db.conn()))
}

#[tauri::command]
pub async fn create_earmark(
    app: AppHandle,
    state: State<'_, AppState>,
    input: EarmarkInput,
) -> AppResult<Earmark> {
    let created = write(&state, "earmark.create", |tx, cmd| {
        earmark::create(tx, cmd, &input)
    })?;
    emit_changed(&app, &["earmark"]);
    Ok(created)
}

#[tauri::command]
pub async fn update_earmark(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    input: EarmarkInput,
) -> AppResult<Earmark> {
    let updated = write(&state, "earmark.update", |tx, cmd| {
        earmark::update(tx, cmd, id, &input)
    })?;
    emit_changed(&app, &["earmark"]);
    Ok(updated)
}

#[tauri::command]
pub async fn list_earmark_entries(
    state: State<'_, AppState>,
    earmark_id: i64,
) -> AppResult<Vec<Entry>> {
    with_db(&state, |db| earmark::entries(db.conn(), earmark_id))
}

#[tauri::command]
pub async fn add_earmark_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    earmark_id: i64,
    input: EntryInput,
) -> AppResult<Entry> {
    let entry = write(&state, "earmark_entry.add", |tx, cmd| {
        earmark::add_entry(tx, cmd, earmark_id, &input)
    })?;
    emit_changed(&app, &["earmark"]);
    Ok(entry)
}

#[tauri::command]
pub async fn delete_earmark_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> AppResult<()> {
    write(&state, "earmark_entry.delete", |tx, cmd| {
        earmark::delete_entry(tx, cmd, id)
    })?;
    emit_changed(&app, &["earmark"]);
    Ok(())
}

/// The next `count` expected dates of a stream or the due dates of an obligation, from today.
#[tauri::command]
pub async fn next_occurrences(
    state: State<'_, AppState>,
    kind: String,
    id: i64,
    count: Option<usize>,
) -> AppResult<Vec<String>> {
    with_db(&state, |db| {
        let today = today(db)?;
        let to = today + chrono::Duration::days(400);
        let dates = match kind.as_str() {
            "income" => income::occurrences(&income::get(db.conn(), id)?, today, to)?,
            "obligation" => obligation::occurrences(&obligation::get(db.conn(), id)?, today, to)?,
            other => {
                return Err(AppError::validation(
                    "kind",
                    format!("{other:?} is neither income nor obligation"),
                ))
            }
        };
        Ok(dates
            .into_iter()
            .take(count.unwrap_or(3))
            .map(crate::dates::format_civil)
            .collect())
    })
}

#[tauri::command]
pub async fn list_policies(state: State<'_, AppState>) -> AppResult<Vec<Policy>> {
    with_db(&state, |db| policy::list(db.conn()))
}

// ---- the hero -----------------------------------------------------------------------------------

/// Safe-to-spend as of today, with every term's rows (ARCHITECTURE §5.4).
#[tauri::command]
pub async fn safe_to_spend(state: State<'_, AppState>) -> AppResult<SafeToSpend> {
    with_db(&state, |db| {
        let today = today(db)?;
        safe::safe_to_spend(db.conn(), today)
    })
}

/// The next confirmed income and the unpaid obligations due within `days` (default 14).
#[tauri::command]
pub async fn upcoming(state: State<'_, AppState>, days: Option<i64>) -> AppResult<Upcoming> {
    with_db(&state, |db| {
        let today = today(db)?;
        safe::upcoming(db.conn(), today, days.unwrap_or(14).clamp(1, 400))
    })
}

// ---- forecast (M5) ---------------------------------------------------------------------------

/// The 91-day daily forecast for a scenario (the baseline when none is given).
#[tauri::command]
pub async fn forecast(
    state: State<'_, AppState>,
    scenario: Option<Scenario>,
) -> AppResult<Forecast> {
    with_db(&state, |db| {
        let today = today(db)?;
        forecast::run(db.conn(), today, &scenario.unwrap_or_default())
    })
}

/// Every variable category's three trailing buckets, median and override.
#[tauri::command]
pub async fn variable_spend_model(state: State<'_, AppState>) -> AppResult<Vec<CategoryModel>> {
    with_db(&state, |db| {
        let today = today(db)?;
        forecast::variable::model(db.conn(), today)
    })
}

/// Replace (or with `null` restore) the per-30-days figure the forecast spends for a category.
#[tauri::command]
pub async fn set_variable_spend_override(
    app: AppHandle,
    state: State<'_, AppState>,
    category_id: i64,
    per_30_days_cents: Option<i64>,
) -> AppResult<Vec<CategoryModel>> {
    let model = with_db(&state, |db| {
        let today = today(db)?;
        let tx = db.conn_mut().transaction()?;
        let cmd = audit::begin(&tx, "forecast.set_variable_override", Actor::User)?;
        forecast::variable::set_override(&tx, &cmd, category_id, per_30_days_cents)?;
        tx.commit()?;
        forecast::variable::model(db.conn(), today)
    })?;
    emit_changed(&app, &["variable_spend_override"]);
    Ok(model)
}

/// Store today's baseline as the plan later forecasts are drawn against.
#[tauri::command]
pub async fn save_forecast_plan(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<PlanOverlay> {
    let plan = with_db(&state, |db| {
        let today = today(db)?;
        let tx = db.conn_mut().transaction()?;
        let cmd = audit::begin(&tx, "forecast.save_plan", Actor::User)?;
        let plan = forecast::save_plan(&tx, &cmd, today)?;
        tx.commit()?;
        Ok(plan)
    })?;
    emit_changed(&app, &["snapshot"]);
    Ok(plan)
}
