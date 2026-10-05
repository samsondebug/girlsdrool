//! Tauri commands: thin wrappers that validate, call the engines, and map errors
//! (ARCHITECTURE §7). Every write runs in one transaction under one command group and emits
//! `kept://changed` so the webview recomputes. Nothing monetary is computed here.

use std::path::PathBuf;

use rusqlite::Transaction;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::config::{self, DataPaths};
use crate::dates::{parse_zone, today_in, CivilDate};
use crate::db::audit::{self, Actor, CommandRecord};
use crate::db::repo::account::{self, Account, AccountPatch, NewAccount};
use crate::db::repo::batch::{self, ImportBatch, QuarantineRow};
use crate::db::repo::category::{self, Category, NewCategory};
use crate::db::repo::ledger::{self, Cursor, LedgerFilter, LedgerPage};
use crate::db::repo::saved_view::{self, SavedView};
use crate::db::repo::txn::{self, SplitPart, TxnPatch, TxnRecord};
use crate::db::settings::{self, Settings};
use crate::db::{Db, OpenMode};
use crate::error::{AppError, AppResult};
use crate::import::profile::{self, Profile};
use crate::import::report::ImportReport;
use crate::import::{self, ImportInput, Preview, QuarantineAction, UndoReport};
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
        let tx = db.conn_mut().transaction()?;
        let cmd = audit::begin(&tx, name, Actor::User)?;
        let out = f(&tx, &cmd)?;
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
        import::resolve_quarantine(db.conn_mut(), id, action)
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
