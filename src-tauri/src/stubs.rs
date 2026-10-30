//! Real dependency wiring (audit fix 4f), plus Noop* fallbacks kept around
//! for reference/tests. `build_deps` is what `commands.rs` calls.

use std::sync::Arc;

use tauri::AppHandle;

use crate::config::Config;
use crate::events::TauriEmitter;
use crate::models::*;
use crate::orchestrator::Deps;
use crate::traits::*;

/* ---------------------------------------------------------------------- */
/* Noop fallbacks (compile-only / test fixtures)                          */
/* ---------------------------------------------------------------------- */

pub struct NoopScriptProcessor;
impl ScriptProcessor for NoopScriptProcessor {
    fn extract_claims(&self, _text: &str) -> Vec<Claim> { Vec::new() }
}

pub struct NoopBrowserWorker;
impl BrowserWorker for NoopBrowserWorker {
    fn send(&self, req: WorkerRequest) -> WorkerResponse {
        WorkerResponse::Error { req_id: req.req_id, message: "no browser worker configured".into() }
    }
}

pub struct NoopEvidenceParser;
impl EvidenceParser for NoopEvidenceParser {
    fn parse(&self, _raw_response: &str, _claim_id: &str) -> Vec<Evidence> { Vec::new() }
    // Checklist item 2: extended to satisfy the audit-fixed trait.
    fn parse_challenge(&self, _raw_response: &str, _claim_id: &str) -> ChallengeOutcome {
        ChallengeOutcome { agrees: true, reason: String::new(), new_evidence: Vec::new() }
    }
}

pub struct NoopCorrectionEngine;
impl CorrectionEngine for NoopCorrectionEngine {
    fn correct(&self, _claim: &Claim, _evidence: &[Evidence]) -> Option<Correction> { None }
}

pub struct NoopResultValidator;
impl ResultValidator for NoopResultValidator {
    fn validate(&self, _result: &FinalResult) -> ValidationOutcome {
        ValidationOutcome { valid: true, unverified_claim_ids: Vec::new(), notes: Vec::new() }
    }
}

/* ---------------------------------------------------------------------- */
/* PromptEngineAdapter — bridges verification_prompts + the browser worker
   to the EvidenceParser and CorrectionEngine traits. Closes gaps G2/G3. */
/* ---------------------------------------------------------------------- */

pub struct PromptEngineAdapter {
    pub worker: Arc<dyn BrowserWorker>,
    pub correction_provider: String,
}

impl EvidenceParser for PromptEngineAdapter {
    fn parse(&self, raw: &str, claim_id: &str) -> Vec<Evidence> {
        crate::verification_prompts::parse_research_response(raw, claim_id)
    }
    fn parse_challenge(&self, raw: &str, _claim_id: &str) -> ChallengeOutcome {
        crate::verification_prompts::parse_challenge_response(raw)
    }
}

impl CorrectionEngine for PromptEngineAdapter {
    fn correct(&self, claim: &Claim, evidence: &[Evidence]) -> Option<Correction> {
        let prompt = crate::verification_prompts::build_correction_prompt(claim, evidence);
        let req = WorkerRequest {
            req_id: crate::orchestrator::next_id("req"),
            provider: self.correction_provider.clone(),
            action: "send_prompt".into(),
            prompt,
            page_id: format!("{}-{}", self.correction_provider, claim.id),
        };
        let raw = match self.worker.send(req) {
            WorkerResponse::Ok { text, .. } => text,
            WorkerResponse::Error { .. } => return None,
        };
        let ev_ids: Vec<String> = evidence.iter().filter_map(|e| e.id.clone()).collect();
        crate::verification_prompts::parse_correction_response(&raw, claim, ev_ids)
    }
}

/* ---------------------------------------------------------------------- */
/* Real build_deps                                                        */
/* ---------------------------------------------------------------------- */

/// `browser_worker` is spawned once in `lib.rs` at app startup (a single
/// persistent Chromium context should not be re-spawned per script) and
/// passed in here.
pub fn build_deps(app: AppHandle, cfg: Arc<Config>, browser_worker: Arc<dyn BrowserWorker>) -> Arc<Deps> {
    let store = Arc::new(
        crate::store::SqliteStore::open("verifier.db").expect("open sqlite store"),
    );

    let prompt_engine = Arc::new(PromptEngineAdapter {
        worker: Arc::clone(&browser_worker),
        correction_provider: cfg.roles.recheck.clone(),
    });

    let script_processor = Arc::new(crate::script_processor::LlmScriptProcessor::new(
        Arc::clone(&browser_worker),
        cfg.roles.research.clone(),
    ));

    Arc::new(Deps {
        script_processor,
        browser_worker,
        evidence_parser: prompt_engine.clone(),
        correction_engine: prompt_engine,
        result_validator: Arc::new(crate::result_validator::RuleValidator),
        store,
        emitter: Arc::new(TauriEmitter::new(app)),
    })
}
