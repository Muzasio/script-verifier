//! Tauri command: open a provider's page in the app's Chromium so the
//! user can run the site's own bulk chat-cleanup flow.

use tauri::State;
use tracing::info;

use crate::models::{WorkerRequest, WorkerResponse};
use crate::orchestrator::next_id;
use crate::AppState;

#[tauri::command]
pub async fn open_provider_page(
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
        req_id: next_id("open"),
        provider: provider.clone(),
        action: "new_page".into(),
        prompt: String::new(),
        page_id: format!("{provider}-maintenance"),
    };

    info!(provider = %provider, "open_provider_page requested");

    let resp = tokio::task::spawn_blocking(move || worker.send(req))
        .await
        .map_err(|e| format!("open page task panicked: {e}"))?;

    match resp {
        WorkerResponse::Ok { text, .. } => Ok(text),
        WorkerResponse::Error { message, .. } => Err(message),
    }
}
