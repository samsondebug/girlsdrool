//! Optional "remember passphrase" (ADR-0012). The passphrase itself is stored — SQLCipher needs
//! it, and the database must stay openable without the credential store — under service `Kept`
//! and a user name derived from the data folder, so two data folders never collide.
//!
//! Windows Credential Manager is the shipping store (bound to the Windows logon via DPAPI). On
//! Linux the kernel keyutils session keyring is used for development. Tests install
//! `keyring_core::mock::Store` before calling into this module; an already-installed default
//! store is always respected.

use std::path::Path;
use std::sync::OnceLock;

use keyring_core::{Entry, Error as KeyringError};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::error::{AppError, AppResult};

pub const SERVICE: &str = "Kept";

/// Credential-store user name for a data folder: hex SHA-256 of its path as given.
pub fn user_for(data_dir: &Path) -> String {
    let canonical = data_dir
        .canonicalize()
        .unwrap_or_else(|_| data_dir.to_path_buf());
    hex::encode(Sha256::digest(canonical.to_string_lossy().as_bytes()))
}

fn ensure_store() -> AppResult<()> {
    if keyring_core::get_default_store().is_some() {
        return Ok(());
    }
    static INIT: OnceLock<Result<(), String>> = OnceLock::new();
    INIT.get_or_init(init_platform_store)
        .clone()
        .map_err(AppError::Unsupported)
}

#[cfg(windows)]
fn init_platform_store() -> Result<(), String> {
    let store = windows_native_keyring_store::Store::new()
        .map_err(|e| format!("Windows Credential Manager is unavailable: {e}"))?;
    keyring_core::set_default_store(store);
    Ok(())
}

#[cfg(target_os = "linux")]
fn init_platform_store() -> Result<(), String> {
    let store = linux_keyutils_keyring_store::Store::new()
        .map_err(|e| format!("kernel keyring is unavailable: {e}"))?;
    keyring_core::set_default_store(store);
    Ok(())
}

#[cfg(not(any(windows, target_os = "linux")))]
fn init_platform_store() -> Result<(), String> {
    Err("no credential store is supported on this platform".to_string())
}

fn entry(data_dir: &Path) -> AppResult<Entry> {
    ensure_store()?;
    Entry::new(SERVICE, &user_for(data_dir)).map_err(map_err)
}

fn map_err(e: KeyringError) -> AppError {
    AppError::Unsupported(format!("credential store: {e}"))
}

/// Store the passphrase for this data folder, replacing any previous one.
pub fn remember(data_dir: &Path, passphrase: &str) -> AppResult<()> {
    entry(data_dir)?.set_password(passphrase).map_err(map_err)
}

/// The remembered passphrase, if any.
pub fn load(data_dir: &Path) -> AppResult<Option<Zeroizing<String>>> {
    match entry(data_dir)?.get_password() {
        Ok(p) => Ok(Some(Zeroizing::new(p))),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(e) => Err(map_err(e)),
    }
}

pub fn is_remembered(data_dir: &Path) -> AppResult<bool> {
    Ok(load(data_dir)?.is_some())
}

/// Remove the remembered passphrase. Succeeds when there was none.
pub fn forget(data_dir: &Path) -> AppResult<()> {
    match entry(data_dir)?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(e) => Err(map_err(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn use_mock_store() {
        if keyring_core::get_default_store().is_none() {
            keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
        }
    }

    #[test]
    fn user_name_is_stable_and_folder_specific() {
        let a = user_for(Path::new("/data/a"));
        let b = user_for(Path::new("/data/b"));
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
        assert_eq!(a, user_for(Path::new("/data/a")));
    }

    #[test]
    fn remember_load_forget_round_trip() {
        use_mock_store();
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(dir.path()).unwrap(), None);
        assert!(!is_remembered(dir.path()).unwrap());
        remember(dir.path(), "correct horse").unwrap();
        assert_eq!(
            load(dir.path()).unwrap().as_deref().map(String::as_str),
            Some("correct horse")
        );
        remember(dir.path(), "battery staple").unwrap();
        assert_eq!(
            load(dir.path()).unwrap().as_deref().map(String::as_str),
            Some("battery staple")
        );
        forget(dir.path()).unwrap();
        forget(dir.path()).unwrap();
        assert_eq!(load(dir.path()).unwrap(), None);
    }
}
