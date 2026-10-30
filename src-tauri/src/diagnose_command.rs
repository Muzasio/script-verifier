//! Tauri command: diagnose_page(provider) -> string dump of DOM info.

use tauri::State;
use tracing::info;

use crate::models::{WorkerRequest, WorkerResponse};
use crate::orchestrator::next_id;
use crate::AppState;

#[tauri::command]
pub async fn diagnose_page(
    state: State<'_, AppState>,
    provider: String,
) -> Result<String, String> {
    let worker = {
        let guard = state.browser_worker.read().await;
        guard
            .clone()
            .ok_or_else(|| "browser worker not initialised".to_string())?
    };

    let req = WorkerRequest {
        req_id: next_id("diag"),
        provider: provider.clone(),
        action: "diagnose_page".into(),
        prompt: String::new(),
        page_id: format!("{provider}-diagnose"),
    };

    info!(provider = %provider, "diagnose_page requested");

    let resp = tokio::task::spawn_blocking(move || worker.send(req))
        .await
        .map_err(|e| format!("diagnose task panicked: {e}"))?;

    match resp {
        WorkerResponse::Ok { text, .. } => Ok(text),
        WorkerResponse::Error { message, .. } => Err(message),
    }
}
