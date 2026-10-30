//! Tauri command: clear_chats(provider) -> number deleted.

use tauri::State;
use tracing::info;

use crate::models::{WorkerRequest, WorkerResponse};
use crate::orchestrator::next_id;
use crate::AppState;

#[tauri::command]
pub async fn clear_chats(
    state: State<'_, AppState>,
    provider: String,
) -> Result<u64, String> {
    let worker = {
        let guard = state.browser_worker.read().await;
        guard
            .clone()
            .ok_or_else(|| "browser worker not initialised".to_string())?
    };

    let req = WorkerRequest {
        req_id: next_id("clean"),
        provider: provider.clone(),
        action: "clear_chats".into(),
        prompt: String::new(),
        page_id: format!("{provider}-cleanup"),
    };

    info!(provider = %provider, "clear_chats requested");

    let resp = tokio::task::spawn_blocking(move || worker.send(req))
        .await
        .map_err(|e| format!("cleanup task panicked: {e}"))?;

    match resp {
        WorkerResponse::Ok { text, .. } => {
            let n: u64 = text.trim().parse().unwrap_or(0);
            info!(provider = %provider, deleted = n, "clear_chats done");
            Ok(n)
        }
        WorkerResponse::Error { message, .. } => Err(message),
    }
}
