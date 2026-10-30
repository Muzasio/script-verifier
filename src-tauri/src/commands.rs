//! Tauri command surface — thin adapters over the orchestrator.

use std::collections::HashMap;
use std::sync::Arc;

use tauri::{AppHandle, State};
use tokio::sync::{Mutex, RwLock};

use crate::orchestrator::{now_iso, ControlFlags, Orchestrator, PipelineStage, SharedState};
use crate::AppState;

pub struct ScriptHandle {
    pub script_id: String,
    pub control: Arc<ControlFlags>,
    pub state: Arc<RwLock<SharedState>>,
    pub join: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

#[tauri::command]
pub async fn start_verification(
    app: AppHandle,
    state: State<'_, AppState>,
    script_text: String,
) -> Result<String, String> {
    let cfg = {
        let guard = state.config.read().await;
        guard.clone().ok_or_else(|| "config not loaded".to_string())?
    };

    let browser_worker = {
        let guard = state.browser_worker.read().await;
        guard.clone().ok_or_else(|| "browser worker not started".to_string())?
    };

    let script_id = format!("s-{}", short_token());
    let run_id = format!("r-{}", short_token());
    let _ = now_iso();

    let deps = crate::stubs::build_deps(app, Arc::clone(&cfg), browser_worker);

    let control = ControlFlags::new();
    let shared = Arc::new(RwLock::new(SharedState {
        script_id: script_id.clone(),
        stage: PipelineStage::Idle,
        message: "starting".into(),
        running: true,
        claims: Vec::new(),
        result: None,
    }));

    let orchestrator = Orchestrator::new(
        script_id.clone(),
        run_id,
        script_text,
        Arc::clone(&cfg),
        deps,
        Arc::clone(&control),
        Arc::clone(&shared),
    );

    let join = tauri::async_runtime::spawn(async move {
        orchestrator.run().await;
    });

    let handle = Arc::new(ScriptHandle {
        script_id: script_id.clone(),
        control,
        state: shared,
        join: Mutex::new(Some(join)),
    });

    state.scripts.lock().await.insert(script_id.clone(), handle);
    Ok(script_id)
}

#[tauri::command]
pub async fn stop_verification(state: State<'_, AppState>, script_id: String) -> Result<(), String> {
    let handle = lookup(&state, &script_id).await?;
    handle.control.stop();
    Ok(())
}

#[tauri::command]
pub async fn get_progress(state: State<'_, AppState>, script_id: String) -> Result<crate::models::ProgressState, String> {
    let handle = lookup(&state, &script_id).await?;
    let s = handle.state.read().await;
    Ok(crate::models::ProgressState {
        script_id: s.script_id.clone(),
        stage: s.stage.as_event_str().to_string(),
        message: s.message.clone(),
        running: s.running,
    })
}

#[tauri::command]
pub async fn get_claims(state: State<'_, AppState>, script_id: String) -> Result<Vec<crate::models::Claim>, String> {
    let handle = lookup(&state, &script_id).await?;
    // Bind before returning: `Ok(handle.state.read().await.claims.clone())`
    // keeps the RwLockReadGuard alive until the end of the statement, but
    // `handle` (which owns the lock) is dropped at the end of the block
    // first — E0597. Binding the clone first drops the guard immediately.
    let claims = handle.state.read().await.claims.clone();
    Ok(claims)
}

#[tauri::command]
pub async fn get_result(state: State<'_, AppState>, script_id: String) -> Result<crate::models::FinalResult, String> {
    let handle = lookup(&state, &script_id).await?;
    let s = handle.state.read().await;
    if let Some(r) = &s.result {
        return Ok(r.clone());
    }
    Ok(crate::models::FinalResult {
        script_id: s.script_id.clone(),
        claims: s.claims.clone(),
        evidence: Vec::new(),
        corrections: Vec::new(),
        final_script: String::new(),
        verdict: "NEEDS_REVIEW".into(),
        unverified_claim_ids: Vec::new(),
    })
}

async fn lookup(state: &State<'_, AppState>, script_id: &str) -> Result<Arc<ScriptHandle>, String> {
    let guard: tokio::sync::MutexGuard<'_, HashMap<String, Arc<ScriptHandle>>> = state.scripts.lock().await;
    guard.get(script_id).cloned().ok_or_else(|| format!("unknown script_id: {script_id}"))
}

fn short_token() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let t = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    format!("{t:x}{n:02x}")
}
