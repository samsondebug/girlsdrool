//! Kept core: every monetary computation lives in this crate. The webview only formats
//! integers this crate returns. Built without the `app` feature, the crate contains the engines
//! and database layer only, so they can be tested on a host without a webview.

pub mod config;
pub mod dates;
pub mod db;
pub mod error;
pub mod export;
pub mod logging;
pub mod money;
pub mod secret;

#[cfg(feature = "app")]
pub mod cmd;

use std::sync::Mutex;

use tracing_appender::non_blocking::WorkerGuard;

use crate::config::DataPaths;
use crate::db::Db;

/// Process-wide state shared with every command. `db` is `None` while locked.
pub struct AppState {
    pub paths: Mutex<Option<DataPaths>>,
    pub db: Mutex<Option<Db>>,
    pub log_guard: Mutex<Option<WorkerGuard>>,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            paths: Mutex::new(None),
            db: Mutex::new(None),
            log_guard: Mutex::new(None),
        }
    }
}

impl AppState {
    /// Read the configured data folder (env or config file) and start logging into it. A
    /// missing or unreadable configuration leaves the app in the "choose a data folder" state.
    pub fn boot() -> Self {
        let state = AppState::default();
        match config::load_data_dir() {
            Ok(Some(dir)) => {
                let paths = DataPaths::new(dir);
                if let Err(e) = state.adopt_paths(paths) {
                    eprintln!("kept: could not use configured data folder: {e}");
                }
            }
            Ok(None) => {}
            Err(e) => eprintln!("kept: could not read configuration: {e}"),
        }
        state
    }

    /// Make `paths` the active data folder: create it and start the log file there.
    pub fn adopt_paths(&self, paths: DataPaths) -> error::AppResult<()> {
        paths.ensure_dirs()?;
        if let Some(guard) = logging::init(&paths.logs)? {
            *self.log_guard.lock().map_err(|_| poisoned())? = Some(guard);
        }
        *self.paths.lock().map_err(|_| poisoned())? = Some(paths);
        Ok(())
    }
}

pub fn poisoned() -> error::AppError {
    error::AppError::Internal("a lock was poisoned by an earlier panic".into())
}

#[cfg(feature = "app")]
pub fn run() {
    let state = AppState::boot();
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            cmd::app_status,
            cmd::choose_data_dir,
            cmd::create_database,
            cmd::unlock,
            cmd::unlock_remembered,
            cmd::lock,
            cmd::remember_passphrase,
            cmd::forget_remembered,
            cmd::get_settings,
            cmd::update_setting,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        tracing::error!(error = %e, "tauri runtime failed");
        eprintln!("kept: tauri runtime failed: {e}");
        std::process::exit(1);
    }
}
