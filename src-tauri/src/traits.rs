//! Trait interfaces for the pipeline stages.

use serde::{Deserialize, Serialize};

use crate::models::{
    ChallengeOutcome, Claim, Correction, Evidence, FinalResult, RunRecord, ScriptRecord,
    VerificationRun, WorkerRequest, WorkerResponse,
};

pub trait ScriptProcessor: Send + Sync + 'static {
    fn extract_claims(&self, text: &str) -> Vec<Claim>;
}

pub trait BrowserWorker: Send + Sync + 'static {
    fn send(&self, req: WorkerRequest) -> WorkerResponse;
}

/// Audit fix 4b: extended with `parse_challenge` so the challenge-stage
/// response (a different JSON shape than research/recheck) has its own
/// parse path instead of being forced through `parse`.
pub trait EvidenceParser: Send + Sync + 'static {
    fn parse(&self, raw_response: &str, claim_id: &str) -> Vec<Evidence>;
    fn parse_challenge(&self, raw_response: &str, claim_id: &str) -> ChallengeOutcome;
}

pub trait CorrectionEngine: Send + Sync + 'static {
    fn correct(&self, claim: &Claim, evidence: &[Evidence]) -> Option<Correction>;
}

pub trait ResultValidator: Send + Sync + 'static {
    fn validate(&self, result: &FinalResult) -> ValidationOutcome;
}

pub trait Store: Send + Sync + 'static {
    fn save_script(&self, id: &str, original_text: &str, created_at: &str);
    fn get_script(&self, id: &str) -> Option<ScriptRecord>;
    fn list_scripts(&self) -> Vec<ScriptRecord>;

    fn save_claim(&self, script_id: &str, c: &Claim);
    fn get_claim(&self, id: &str) -> Option<Claim>;
    fn list_claims(&self, script_id: &str) -> Vec<Claim>;

    fn save_evidence(&self, e: &Evidence);
    fn get_evidence(&self, id: &str) -> Option<Evidence>;
    fn list_evidence(&self, claim_id: &str) -> Vec<Evidence>;

    fn save_verification_run(&self, r: &VerificationRun);
    fn get_verification_run(&self, id: &str) -> Option<VerificationRun>;
    fn list_verification_runs(&self, claim_id: &str) -> Vec<VerificationRun>;

    fn save_correction(&self, c: &Correction);
    fn list_corrections(&self, claim_id: &str) -> Vec<Correction>;

    fn save_run(&self, r: &RunRecord);
    fn get_run(&self, id: &str) -> Option<RunRecord>;
    fn list_runs(&self, script_id: &str) -> Vec<RunRecord>;
    fn update_run_status(&self, id: &str, status: &str, finished_at: Option<&str>);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationOutcome {
    pub valid: bool,
    pub unverified_claim_ids: Vec<String>,
    pub notes: Vec<String>,
}
