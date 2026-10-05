//! Local rotating log under the data folder (ADR-0015). Logs carry ids, counts, durations and
//! error kinds — never amounts, payees, memos, counterparties, passphrases or key material.

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;

use crate::error::{AppError, AppResult};

pub const LOG_ENV: &str = "KEPT_LOG";
pub const MAX_LOG_FILES: usize = 14;

/// Install the global subscriber writing daily files `kept.YYYY-MM-DD.log` into `logs_dir`.
/// Returns the guard that flushes on drop; keep it alive for the life of the process. Returns
/// `Ok(None)` when a subscriber is already installed (a second call in the same process).
pub fn init(logs_dir: &Path) -> AppResult<Option<WorkerGuard>> {
    std::fs::create_dir_all(logs_dir).map_err(|e| AppError::io(logs_dir, e))?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("kept")
        .filename_suffix("log")
        .max_log_files(MAX_LOG_FILES)
        .build(logs_dir)
        .map_err(|e| AppError::Internal(format!("log appender: {e}")))?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = EnvFilter::try_from_env(LOG_ENV).unwrap_or_else(|_| EnvFilter::new("info"));
    let installed = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .with_target(true)
        .try_init()
        .is_ok();
    if installed {
        tracing::info!(version = env!("CARGO_PKG_VERSION"), "kept logging started");
        Ok(Some(guard))
    } else {
        Ok(None)
    }
}
