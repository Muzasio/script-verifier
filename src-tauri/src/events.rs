//! Event emission surface. The orchestrator talks to `EventEmitter`; the
//! concrete `TauriEmitter` fans out to the window via Tauri's event system.

use tauri::{AppHandle, Emitter};

use crate::models::Claim;

pub const PROGRESS_EVENT: &str = "verification://progress";
pub const CLAIM_UPDATE_EVENT: &str = "verification://claim_update";

pub trait EventEmitter: Send + Sync + 'static {
    fn emit_progress(&self, stage: &str, message: &str);
    fn emit_claim_update(&self, claim: &Claim);
}

#[derive(serde::Serialize, Clone)]
struct ProgressPayload<'a> {
    stage: &'a str,
    message: &'a str,
}

pub struct TauriEmitter {
    app: AppHandle,
}

impl TauriEmitter {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl EventEmitter for TauriEmitter {
    fn emit_progress(&self, stage: &str, message: &str) {
        let payload = ProgressPayload { stage, message };
        if let Err(e) = self.app.emit(PROGRESS_EVENT, payload) {
            tracing::warn!(error = %e, event = PROGRESS_EVENT, "emit failed");
        }
    }

    fn emit_claim_update(&self, claim: &Claim) {
        if let Err(e) = self.app.emit(CLAIM_UPDATE_EVENT, claim.clone()) {
            tracing::warn!(error = %e, event = CLAIM_UPDATE_EVENT, "emit failed");
        }
    }
}
