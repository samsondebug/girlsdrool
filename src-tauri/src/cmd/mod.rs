//! Tauri commands: thin wrappers that validate, call the engines, and map errors
//! (ARCHITECTURE §7). Every write emits `kept://changed` so the webview recomputes.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::config::{self, DataPaths};
use crate::db::settings::{self, Settings};
use crate::db::{Db, OpenMode};
use crate::error::{AppError, AppResult};
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

/// Create `kept.db` under a new passphrase and open it.
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

/// Unlock with the passphrase held by the credential store, if any.
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

/// Store the passphrase of the currently unlocked database in the credential store.
#[tauri::command]
pub async fn remember_passphrase(state: State<'_, AppState>) -> AppResult<AppStatus> {
    let paths = current_paths(&state)?;
    {
        let guard = state.db.lock().map_err(|_| poisoned())?;
        let db = guard.as_ref().ok_or(AppError::Locked)?;
        secret::remember(&paths.root, db.passphrase())?;
    }
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
    let guard = state.db.lock().map_err(|_| poisoned())?;
    let db = guard.as_ref().ok_or(AppError::Locked)?;
    settings::load(db.conn())
}

#[tauri::command]
pub async fn update_setting(
    app: AppHandle,
    state: State<'_, AppState>,
    key: String,
    value: serde_json::Value,
) -> AppResult<Settings> {
    let updated = {
        let mut guard = state.db.lock().map_err(|_| poisoned())?;
        let db = guard.as_mut().ok_or(AppError::Locked)?;
        settings::update(db.conn_mut(), &key, &value)?
    };
    emit_changed(&app, &["setting"]);
    Ok(updated)
}
