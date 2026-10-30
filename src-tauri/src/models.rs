//! Wire types. Field names are snake_case to match the SHARED CONTRACTS
//! JSON shapes byte-for-byte; serde serialises them verbatim.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub id: String,
    pub text: String,
    pub original_wording: String,
    pub context: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub claim_id: String,
    pub source_url: String,
    pub source_title: String,
    pub excerpt: String,
    pub source_tier: String,
    pub verdict: String,
    pub provider: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Correction {
    pub claim_id: String,
    pub original_text: String,
    pub corrected_text: String,
    pub reason: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalResult {
    pub script_id: String,
    pub claims: Vec<Claim>,
    pub evidence: Vec<Evidence>,
    pub corrections: Vec<Correction>,
    pub final_script: String,
    pub verdict: String,
    pub unverified_claim_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerRequest {
    pub req_id: String,
    pub provider: String,
    pub action: String,
    pub prompt: String,
    pub page_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum WorkerResponse {
    Ok { req_id: String, text: String },
    Error { req_id: String, message: String },
}

impl WorkerResponse {
    pub fn is_ok(&self) -> bool {
        matches!(self, WorkerResponse::Ok { .. })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressState {
    pub script_id: String,
    pub stage: String,
    pub message: String,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptRecord {
    pub id: String,
    pub original_text: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRun {
    pub id: String,
    pub claim_id: String,
    pub stage: String,
    pub provider: String,
    pub input_prompt: String,
    pub raw_response: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: String,
    pub script_id: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
}

/// Audit fix 4a: lives here so `traits.rs` can reference it without
/// importing `verification_prompts`. `verification_prompts.rs` re-exports
/// this via `pub use crate::models::ChallengeOutcome;` instead of defining
/// its own copy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeOutcome {
    pub agrees: bool,
    pub reason: String,
    pub new_evidence: Vec<Evidence>,
}
