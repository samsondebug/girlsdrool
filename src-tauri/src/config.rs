//! Portable layout (ADR-0007): `Kept.exe` + `kept.config.json` beside it naming the data folder.
//! `KEPT_DATA_DIR` overrides the config file (tests, portable drives).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

pub const CONFIG_FILE_NAME: &str = "kept.config.json";
pub const DATA_DIR_ENV: &str = "KEPT_DATA_DIR";
pub const DB_FILE_NAME: &str = "kept.db";

/// Everything that lives under the data folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataPaths {
    pub root: PathBuf,
    pub db: PathBuf,
    pub logs: PathBuf,
    pub backups: PathBuf,
    pub exports: PathBuf,
}

impl DataPaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root: PathBuf = root.into();
        DataPaths {
            db: root.join(DB_FILE_NAME),
            logs: root.join("logs"),
            backups: root.join("backups"),
            exports: root.join("exports"),
            root,
        }
    }

    /// Create the folder tree. Idempotent.
    pub fn ensure_dirs(&self) -> AppResult<()> {
        for dir in [&self.root, &self.logs, &self.backups, &self.exports] {
            std::fs::create_dir_all(dir).map_err(|e| AppError::io(dir, e))?;
        }
        Ok(())
    }

    pub fn db_exists(&self) -> bool {
        self.db.is_file()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    pub data_dir: PathBuf,
}

/// `kept.config.json` beside the running executable.
pub fn config_path() -> AppResult<PathBuf> {
    let exe = std::env::current_exe().map_err(|e| AppError::io("<current_exe>", e))?;
    let dir = exe
        .parent()
        .ok_or_else(|| AppError::Internal("executable has no parent directory".into()))?;
    Ok(dir.join(CONFIG_FILE_NAME))
}

/// Where the data folder is, if it has been chosen: the env override first, then the config file.
pub fn load_data_dir() -> AppResult<Option<PathBuf>> {
    if let Some(dir) = std::env::var_os(DATA_DIR_ENV) {
        if !dir.is_empty() {
            return Ok(Some(PathBuf::from(dir)));
        }
    }
    let path = config_path()?;
    read_config(&path).map(|c| c.map(|c| c.data_dir))
}

pub fn read_config(path: &Path) -> AppResult<Option<AppConfig>> {
    match std::fs::read(path) {
        Ok(bytes) => {
            let cfg: AppConfig =
                serde_json::from_slice(&bytes).map_err(|e| AppError::Validation {
                    field: "config".into(),
                    message: format!("{} is not a valid Kept config: {e}", path.display()),
                })?;
            Ok(Some(cfg))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(AppError::io(path, e)),
    }
}

pub fn write_config(path: &Path, cfg: &AppConfig) -> AppResult<()> {
    let json = serde_json::to_vec_pretty(cfg)?;
    std::fs::write(path, json).map_err(|e| AppError::io(path, e))
}

/// Remember the chosen data folder. With the env override active there is nothing to persist.
pub fn save_data_dir(dir: &Path) -> AppResult<()> {
    if std::env::var_os(DATA_DIR_ENV).is_some_and(|v| !v.is_empty()) {
        return Ok(());
    }
    let path = config_path()?;
    write_config(
        &path,
        &AppConfig {
            data_dir: dir.to_path_buf(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_paths_layout() {
        let p = DataPaths::new("/data/kept");
        assert_eq!(p.db, PathBuf::from("/data/kept/kept.db"));
        assert_eq!(p.logs, PathBuf::from("/data/kept/logs"));
        assert_eq!(p.backups, PathBuf::from("/data/kept/backups"));
        assert_eq!(p.exports, PathBuf::from("/data/kept/exports"));
    }

    #[test]
    fn config_round_trip_and_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE_NAME);
        assert_eq!(read_config(&path).unwrap(), None);
        let cfg = AppConfig {
            data_dir: dir.path().join("data"),
        };
        write_config(&path, &cfg).unwrap();
        assert_eq!(read_config(&path).unwrap(), Some(cfg));
        std::fs::write(&path, b"not json").unwrap();
        assert!(matches!(
            read_config(&path),
            Err(AppError::Validation { .. })
        ));
    }

    #[test]
    fn ensure_dirs_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let p = DataPaths::new(dir.path().join("kept"));
        p.ensure_dirs().unwrap();
        p.ensure_dirs().unwrap();
        assert!(p.logs.is_dir() && p.backups.is_dir() && p.exports.is_dir());
        assert!(!p.db_exists());
    }
}
