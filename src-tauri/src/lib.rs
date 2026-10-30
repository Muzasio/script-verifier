mod browser_worker;
mod cleanup_command;
mod commands;
mod diag_file_command;
mod diagnose_command;
mod config;
mod evidence_manager;
mod events;
mod logging;
mod models;
mod open_page;
mod orchestrator;
mod result_validator;
mod run_log;
mod script_processor;
mod store;
mod stubs;
mod traits;
mod verification_prompts;

use std::collections::HashMap;
use std::sync::Arc;

use tauri::Manager;
use tokio::sync::{Mutex, RwLock};

use browser_worker::ProcessBrowserWorker;
use traits::BrowserWorker;

pub use commands::ScriptHandle;

#[derive(Default)]
pub struct AppState {
    pub scripts: Mutex<HashMap<String, Arc<ScriptHandle>>>,
    pub config: RwLock<Option<Arc<config::Config>>>,
    pub browser_worker: RwLock<Option<Arc<dyn BrowserWorker>>>,
}

pub fn run() {
    logging::init();
    tracing::info!("script-verifier starting");

    tauri::Builder::default()
        .manage(AppState::default())
        .setup(|app| {
            let handle = app.handle().clone();

            let cfg = match config::load_from_repo_root() {
                Ok(c) => {
                    tracing::info!(providers = ?c.providers, "config loaded");
                    c
                }
                Err(e) => {
                    tracing::warn!(error = %e, "config.json not found or invalid, using defaults");
                    config::default_config()
                }
            };
            let cfg = Arc::new(cfg);

            // Spawn the Node browser-worker process once, for the app's
            // lifetime. Path assumes the sibling `browser-worker/` layout:
            //   script-verifier/
            //     src-tauri/    <- this binary's cwd during `tauri dev`
            //     browser-worker/dist/main.js
            let worker: Option<Arc<dyn BrowserWorker>> = match ProcessBrowserWorker::spawn(
                "node",
                "dist/main.js",
                "../browser-worker",
                cfg.timeouts_ms.response_wait,
            ) {
                Ok(w) => Some(Arc::new(w) as Arc<dyn BrowserWorker>),
                Err(e) => {
                    tracing::error!(error = %e, "failed to spawn browser worker — build it first with: cd browser-worker && npm run build");
                    None
                }
            };

            tauri::async_runtime::block_on(async {
                let state: tauri::State<AppState> = handle.state();
                *state.config.write().await = Some(cfg);
                *state.browser_worker.write().await = worker;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::start_verification,
            commands::stop_verification,
            commands::get_progress,
            commands::get_claims,
            commands::get_result,
            open_page::open_provider_page,
            cleanup_command::clear_chats,
            diagnose_command::diagnose_page,
            diag_file_command::write_diag_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
