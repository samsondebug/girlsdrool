//! Kept core: every monetary computation lives in this crate. The webview only formats
//! integers this crate returns. Built without the `app` feature, the crate contains the engines
//! and database layer only, so they can be tested on a host without a webview.

pub mod cash;
pub mod config;
pub mod dates;
pub mod db;
pub mod debt;
pub mod error;
pub mod export;
pub mod forecast;
pub mod import;
pub mod logging;
pub mod money;
pub mod plan;
pub mod review;
pub mod rules;
pub mod secret;
pub mod venture;

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
            cmd::list_accounts,
            cmd::create_account,
            cmd::update_account,
            cmd::list_categories,
            cmd::create_category,
            cmd::rename_category,
            cmd::archive_category,
            cmd::list_import_profiles,
            cmd::create_import_profile,
            cmd::update_import_profile,
            cmd::delete_import_profile,
            cmd::draft_import_profile,
            cmd::test_import_profile,
            cmd::list_backups,
            cmd::backup_now,
            cmd::restore_stage,
            cmd::restore_discard,
            cmd::restore_confirm,
            cmd::change_passphrase,
            cmd::export_full,
            cmd::export_audit_pack,
            cmd::import_preview,
            cmd::import_commit,
            cmd::undo_import_batch,
            cmd::list_import_batches,
            cmd::list_quarantine,
            cmd::resolve_quarantine,
            cmd::ledger_query,
            cmd::ledger_children,
            cmd::get_txn,
            cmd::update_txn,
            cmd::recategorize,
            cmd::split_txn,
            cmd::unsplit_txn,
            cmd::list_saved_views,
            cmd::save_view,
            cmd::delete_saved_view,
            cmd::list_rules,
            cmd::create_rule,
            cmd::update_rule,
            cmd::delete_rule,
            cmd::reorder_rules,
            cmd::apply_rules,
            cmd::propose_rule,
            cmd::link_candidates,
            cmd::link_details,
            cmd::link_transfer,
            cmd::unlink_transfer,
            cmd::link_refund,
            cmd::unlink_refund,
            cmd::acknowledge_firewall,
            cmd::review_queue,
            cmd::spending_view,
            cmd::cash_view,
            cmd::compare_views,
            cmd::list_ventures,
            cmd::create_venture,
            cmd::update_venture,
            cmd::list_reconciliations,
            cmd::reconcile,
            cmd::delete_reconciliation,
            cmd::difference_explorer,
            cmd::trust_status,
            cmd::list_income_streams,
            cmd::create_income_stream,
            cmd::update_income_stream,
            cmd::list_receipts,
            cmd::record_receipt,
            cmd::remove_receipt,
            cmd::list_obligations,
            cmd::create_obligation,
            cmd::update_obligation,
            cmd::set_obligation_status,
            cmd::delete_obligation_candidate,
            cmd::detect_obligation_candidates,
            cmd::list_payments,
            cmd::record_payment,
            cmd::remove_payment,
            cmd::list_earmarks,
            cmd::create_earmark,
            cmd::update_earmark,
            cmd::list_earmark_entries,
            cmd::add_earmark_entry,
            cmd::delete_earmark_entry,
            cmd::next_occurrences,
            cmd::list_policies,
            cmd::safe_to_spend,
            cmd::upcoming,
            cmd::forecast,
            cmd::variable_spend_model,
            cmd::set_variable_spend_override,
            cmd::save_forecast_plan,
            cmd::list_debts,
            cmd::create_debt,
            cmd::update_debt,
            cmd::list_debt_payments,
            cmd::record_debt_payment,
            cmd::remove_debt_payment,
            cmd::debt_payment_candidates,
            cmd::list_informal_loans,
            cmd::create_informal_loan,
            cmd::update_informal_loan,
            cmd::set_informal_note,
            cmd::add_informal_schedule_row,
            cmd::delete_informal_schedule_row,
            cmd::debt_comparison,
            cmd::debt_totals,
            cmd::venture_summary,
            cmd::current_review,
            cmd::start_review,
            cmd::refresh_review,
            cmd::set_review_actions,
            cmd::complete_review,
            cmd::abandon_review,
            cmd::list_reviews,
            cmd::set_review_action_done,
            cmd::take_snapshot,
            cmd::list_snapshots,
            cmd::list_trends,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        tracing::error!(error = %e, "tauri runtime failed");
        eprintln!("kept: tauri runtime failed: {e}");
        std::process::exit(1);
    }
}
