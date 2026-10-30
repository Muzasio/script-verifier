//! Explicit state machine driving the script verification pipeline.
//! Audit fixes 4c (challenge/recheck routing via TaskResult::Challenge)
//! and 4d (config.retries_u32()) are baked in below.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::task::JoinSet;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::events::EventEmitter;
use crate::models::*;
use crate::traits::*;
use crate::run_log::RunLog;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStage {
    Idle,
    ExtractingClaims,
    Researching,
    Challenging,
    Rechecking,
    Correcting,
    FinalVerification,
    Persisting,
    Done,
    Stopped,
    Failed,
}

impl PipelineStage {
    pub fn as_event_str(self) -> &'static str {
        use PipelineStage::*;
        match self {
            Idle => "idle",
            ExtractingClaims => "extracting_claims",
            Researching => "researching",
            Challenging => "challenging",
            Rechecking => "rechecking",
            Correcting => "correcting",
            FinalVerification => "final_verification",
            Persisting => "persisting",
            Done => "done",
            Stopped => "stopped",
            Failed => "error",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, PipelineStage::Done | PipelineStage::Stopped | PipelineStage::Failed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedState {
    pub script_id: String,
    pub stage: PipelineStage,
    pub message: String,
    pub running: bool,
    pub claims: Vec<Claim>,
    pub result: Option<FinalResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClaimPhase {
    AwaitingResearch,
    AwaitingChallenge,
    AwaitingRecheck,
    AwaitingCorrection,
    Complete,
    Failed,
}

#[derive(Debug, Clone)]
struct ClaimState {
    claim: Claim,
    phase: ClaimPhase,
    evidence: Vec<Evidence>,
    correction: Option<Correction>,
    // Audit fix 4c(i): holds the outcome of the challenge pass so the
    // recheck prompt can reference the objection.
    challenge: Option<ChallengeOutcome>,
    in_flight: bool,
}

#[derive(Debug, Default)]
pub struct ControlFlags {
    paused: AtomicBool,
    stopped: AtomicBool,
}

impl ControlFlags {
    pub fn new() -> Arc<Self> { Arc::new(Self::default()) }
    pub fn is_paused(&self) -> bool { self.paused.load(Ordering::SeqCst) }
    pub fn is_stopped(&self) -> bool { self.stopped.load(Ordering::SeqCst) }
    pub fn pause(&self) { self.paused.store(true, Ordering::SeqCst); }
    pub fn resume(&self) { self.paused.store(false, Ordering::SeqCst); }
    pub fn stop(&self) { self.stopped.store(true, Ordering::SeqCst); }
}

pub struct Deps {
    pub script_processor: Arc<dyn ScriptProcessor>,
    pub browser_worker: Arc<dyn BrowserWorker>,
    pub evidence_parser: Arc<dyn EvidenceParser>,
    pub correction_engine: Arc<dyn CorrectionEngine>,
    pub result_validator: Arc<dyn ResultValidator>,
    pub store: Arc<dyn Store>,
    pub emitter: Arc<dyn EventEmitter>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskKind {
    Research,
    Challenge,
    Recheck,
    Correction,
}

struct TaskOutcome {
    claim_id: String,
    kind: TaskKind,
    result: TaskResult,
}

// Audit fix 4c(iii): new Challenge(ChallengeOutcome) variant so a challenge
// response is not force-parsed as Evidence.
enum TaskResult {
    Evidence(Vec<Evidence>),
    Challenge(ChallengeOutcome),
    Correction(Option<Correction>),
    Fatal(String),
}

#[derive(Debug)]
enum StepOutcome {
    Advanced,
    Waiting,
    Done,
}

pub struct Orchestrator {
    script_id: String,
    run_id: String,
    original_text: String,
    stage: PipelineStage,
    message: String,
    claims: Vec<ClaimState>,
    all_evidence: Vec<Evidence>,
    corrections: Vec<Correction>,
    final_result: Option<FinalResult>,
    config: Arc<Config>,
    deps: Arc<Deps>,
    control: Arc<ControlFlags>,
    shared: Arc<RwLock<SharedState>>,
    in_flight: JoinSet<TaskOutcome>,
    started_at: String,
    run_log: Option<Arc<RunLog>>,
}

impl Orchestrator {
    pub fn new(
        script_id: String,
        run_id: String,
        original_text: String,
        config: Arc<Config>,
        deps: Arc<Deps>,
        control: Arc<ControlFlags>,
        shared: Arc<RwLock<SharedState>>,
    ) -> Self {
        let run_log_local = RunLog::create(&script_id).map(Arc::new);

        Self {
            script_id,
            run_id,
            original_text,
            stage: PipelineStage::Idle,
            message: "idle".into(),
            claims: Vec::new(),
            all_evidence: Vec::new(),
            corrections: Vec::new(),
            final_result: None,
            config,
            deps,
            control,
            shared,
            in_flight: JoinSet::new(),
            started_at: now_iso(),
            run_log: run_log_local,
        }
    }

    /// Append one line to the per-run log, if it exists. Cheap, non-fatal.
    fn runlog(&self, msg: &str) {
        if let Some(log) = &self.run_log {
            log.write(msg);
        }
    }

    pub fn deps(&self) -> &Deps { &self.deps }
    pub fn original_text(&self) -> &str { &self.original_text }
    pub fn run_id(&self) -> &str { &self.run_id }
    pub fn started_at(&self) -> &str { &self.started_at }

    pub async fn run(mut self) {
        info!(script_id = %self.script_id, run_id = %self.run_id, "orchestrator started");

        self.persist_run_start().await;
        self.runlog(&format!("run started script_id={} run_id={}", self.script_id, self.run_id));

        let result = self.run_loop().await;

        while let Some(res) = self.in_flight.join_next().await {
            self.apply_task_outcome(res).await;
        }

        match result {
            Ok(()) => {
                if self.control.is_stopped() {
                    self.set_stage(PipelineStage::Stopped, "stopped by user").await;
                } else {
                    self.set_stage(PipelineStage::Done, "verification complete").await;
                }
            }
            Err(e) => {
                error!(script_id = %self.script_id, error = %e, "pipeline failed");
                self.set_stage(PipelineStage::Failed, &format!("error: {e}")).await;
            }
        }

        self.mark_not_running().await;
        self.runlog(&format!("run finished stage={:?}", self.stage));
        self.persist_run_finish().await;

        info!(script_id = %self.script_id, stage = ?self.stage, "orchestrator finished");
    }

    async fn run_loop(&mut self) -> Result<(), String> {
        loop {
            if self.stage.is_terminal() { return Ok(()); }
            if self.control.is_stopped() {
                info!(script_id = %self.script_id, "stop requested");
                return Ok(());
            }
            if self.control.is_paused() {
                if let Some(res) = self.in_flight.join_next().await {
                    self.apply_task_outcome(res).await;
                    continue;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }

            match self.step().await? {
                StepOutcome::Advanced | StepOutcome::Waiting => continue,
                StepOutcome::Done => return Ok(()),
            }
        }
    }

    async fn step(&mut self) -> Result<StepOutcome, String> {
        match self.stage {
            PipelineStage::Idle => {
                self.set_stage(PipelineStage::ExtractingClaims, "extracting claims").await;
                Ok(StepOutcome::Advanced)
            }
            PipelineStage::ExtractingClaims => {
                self.do_extract_claims().await?;
                if self.claims.is_empty() {
                    self.set_stage(PipelineStage::FinalVerification, "final verification").await;
                } else {
                    self.set_stage(PipelineStage::Researching, "researching").await;
                }
                Ok(StepOutcome::Advanced)
            }
            PipelineStage::Researching
            | PipelineStage::Challenging
            | PipelineStage::Rechecking
            | PipelineStage::Correcting => self.step_per_claim_stage().await,
            PipelineStage::FinalVerification => {
                self.do_final_verification().await?;
                self.set_stage(PipelineStage::Persisting, "persisting").await;
                Ok(StepOutcome::Advanced)
            }
            PipelineStage::Persisting => {
                self.do_persist().await?;
                Ok(StepOutcome::Done)
            }
            PipelineStage::Done | PipelineStage::Stopped | PipelineStage::Failed => Ok(StepOutcome::Done),
        }
    }

    async fn step_per_claim_stage(&mut self) -> Result<StepOutcome, String> {
        let mut progressed = false;
        while let Some(res) = self.in_flight.try_join_next() {
            self.apply_task_outcome(res).await;
            progressed = true;
        }
        if progressed { return Ok(StepOutcome::Advanced); }

        let pending = self.pending_claim_ids();

        if pending.is_empty() && self.in_flight.is_empty() {
            self.advance_stage().await;
            return Ok(StepOutcome::Advanced);
        }

        let at_capacity = self.in_flight.len() >= self.config.max_concurrency_per_provider;
        if !pending.is_empty() && !at_capacity {
            let claim_id = pending[0].clone();
            self.spawn_claim_task(claim_id);
            return Ok(StepOutcome::Advanced);
        }

        if !self.in_flight.is_empty() {
            if let Some(res) = self.in_flight.join_next().await {
                self.apply_task_outcome(res).await;
            }
        }
        Ok(StepOutcome::Waiting)
    }

    fn pending_claim_ids(&self) -> Vec<String> {
        let target = match self.stage {
            PipelineStage::Researching => ClaimPhase::AwaitingResearch,
            PipelineStage::Challenging => ClaimPhase::AwaitingChallenge,
            PipelineStage::Rechecking => ClaimPhase::AwaitingRecheck,
            PipelineStage::Correcting => ClaimPhase::AwaitingCorrection,
            _ => return Vec::new(),
        };
        self.claims.iter().filter(|cs| cs.phase == target && !cs.in_flight).map(|cs| cs.claim.id.clone()).collect()
    }

    async fn advance_stage(&mut self) {
        let next: (PipelineStage, &str) = match self.stage {
            PipelineStage::Researching => (PipelineStage::Challenging, "challenging"),
            PipelineStage::Challenging => {
                let any_recheck = self.claims.iter().any(|cs| cs.phase == ClaimPhase::AwaitingRecheck);
                if any_recheck { (PipelineStage::Rechecking, "rechecking") } else { (PipelineStage::Correcting, "correcting") }
            }
            PipelineStage::Rechecking => (PipelineStage::Correcting, "correcting"),
            PipelineStage::Correcting => (PipelineStage::FinalVerification, "final verification"),
            _ => return,
        };
        self.set_stage(next.0, next.1).await;
    }

    async fn do_extract_claims(&mut self) -> Result<(), String> {
        let sp = Arc::clone(&self.deps.script_processor);
        let text = self.original_text.clone();
        let claims = tokio::task::spawn_blocking(move || sp.extract_claims(&text))
            .await
            .map_err(|e| format!("extract_claims panicked: {e}"))?;

        debug!(script_id = %self.script_id, n = claims.len(), "claims extracted");
        self.runlog(&format!("extracted {} claims", claims.len()));

        self.claims = claims.into_iter().map(|c| ClaimState {
            claim: c,
            phase: ClaimPhase::AwaitingResearch,
            evidence: Vec::new(),
            correction: None,
            challenge: None,
            in_flight: false,
        }).collect();

        let store = Arc::clone(&self.deps.store);
        let script_id = self.script_id.clone();
        let claims_for_store: Vec<Claim> = self.claims.iter().map(|cs| cs.claim.clone()).collect();
        let _ = tokio::task::spawn_blocking(move || {
            for c in &claims_for_store { store.save_claim(&script_id, c); }
        }).await;

        let mut s = self.shared.write().await;
        s.claims = self.claims.iter().map(|cs| cs.claim.clone()).collect();
        Ok(())
    }

    fn spawn_claim_task(&mut self, claim_id: String) {
        let kind = match self.stage {
            PipelineStage::Researching => TaskKind::Research,
            PipelineStage::Challenging => TaskKind::Challenge,
            PipelineStage::Rechecking => TaskKind::Recheck,
            PipelineStage::Correcting => TaskKind::Correction,
            _ => return,
        };

        let idx = match self.claims.iter().position(|cs| cs.claim.id == claim_id) {
            Some(i) => i,
            None => return,
        };

        self.claims[idx].in_flight = true;
        self.runlog(&format!("spawn {} claim={}", format!("{:?}", kind).to_lowercase(), claim_id));
        self.claims[idx].claim.status = "researching".into();

        let claim = self.claims[idx].claim.clone();
        let evidence = self.claims[idx].evidence.clone();
        let challenge = self.claims[idx].challenge.clone();
        let deps = Arc::clone(&self.deps);
        let config = Arc::clone(&self.config);
        let script_id = self.script_id.clone();

        debug!(script_id = %script_id, claim = %claim.id, ?kind, "spawn task");

        self.in_flight.spawn(async move {
            let result = run_claim_task(deps, config, script_id, claim.clone(), evidence, challenge, kind).await;
            TaskOutcome { claim_id: claim.id, kind, result }
        });
    }

    async fn apply_task_outcome(&mut self, res: Result<TaskOutcome, tokio::task::JoinError>) {
        let outcome = match res {
            Ok(o) => o,
            Err(e) => { warn!(error = %e, "task join error"); return; }
        };

        let idx = match self.claims.iter().position(|cs| cs.claim.id == outcome.claim_id) {
            Some(i) => i,
            None => return,
        };
        self.claims[idx].in_flight = false;

        match outcome.result {
            TaskResult::Evidence(evidence) => {
                for mut e in evidence {
                    if e.id.as_deref().map_or(true, |s| s.is_empty()) { e.id = Some(next_id("ev")); }
                    self.claims[idx].evidence.push(e.clone());
                    self.all_evidence.push(e);
                }
                self.runlog(&format!("claim {} evidence={} => phase {:?}", outcome.claim_id, self.claims[idx].evidence.len(), next_phase_hint(outcome.kind)));
                let next = match outcome.kind {
                    TaskKind::Research => ClaimPhase::AwaitingChallenge,
                    TaskKind::Recheck => ClaimPhase::AwaitingCorrection,
                    // Challenge no longer produces TaskResult::Evidence after
                    // audit fix 4c; kept for match exhaustiveness only.
                    TaskKind::Challenge | TaskKind::Correction => ClaimPhase::AwaitingCorrection,
                };
                self.claims[idx].phase = next;
                self.recompute_claim_status(idx);
                self.emit_claim_update(idx).await;
            }
            // Audit fix 4c(iii): dedicated Challenge arm. Recheck-vs-correct
            // routing is driven by `outcome.agrees`, matching the spec's
            // "disputed claims go back to a provider" semantics.
            TaskResult::Challenge(outcome) => {
                let needs_recheck = !outcome.agrees;
                let claim_id_for_evidence = self.claims[idx].claim.id.clone();
                for mut e in outcome.new_evidence.clone() {
                    if e.id.as_deref().map_or(true, |s| s.is_empty()) { e.id = Some(next_id("ev")); }
                    e.claim_id = claim_id_for_evidence.clone();
                    self.claims[idx].evidence.push(e.clone());
                    self.all_evidence.push(e);
                }
                self.claims[idx].challenge = Some(outcome);
                self.claims[idx].phase = if needs_recheck { ClaimPhase::AwaitingRecheck } else { ClaimPhase::AwaitingCorrection };
                self.recompute_claim_status(idx);
                self.emit_claim_update(idx).await;
            }
            TaskResult::Correction(corr) => {
                if let Some(c) = corr {
                    self.corrections.push(c.clone());
                    self.claims[idx].correction = Some(c);
                }
                self.claims[idx].phase = ClaimPhase::Complete;
                self.recompute_claim_status(idx);
                self.emit_claim_update(idx).await;
            }
            TaskResult::Fatal(msg) => {
                warn!(claim = %outcome.claim_id, msg = %msg, "claim failed");
                self.claims[idx].phase = ClaimPhase::Failed;
                self.claims[idx].claim.status = "unverified".into();
                self.runlog(&format!("claim {} FAILED: {}", outcome.claim_id, msg));
                self.emit_claim_update(idx).await;
            }
        }
    }

    fn recompute_claim_status(&mut self, idx: usize) {
        let phase = self.claims[idx].phase;
        let has_correction = self.claims[idx].correction.is_some();
        let supports = self.claims[idx].evidence.iter().filter(|e| e.verdict == "supports").count();
        let contradicts = self.claims[idx].evidence.iter().filter(|e| e.verdict == "contradicts").count();

        let status: &str = if phase == ClaimPhase::Failed {
            "unverified"
        } else if has_correction {
            "corrected"
        } else if phase == ClaimPhase::Complete {
            if contradicts > 0 { "contradicted" } else if supports > 0 { "verified" } else { "unverified" }
        } else {
            "researching"
        };
        self.claims[idx].claim.status = status.into();
    }

    async fn do_final_verification(&mut self) -> Result<(), String> {
        let claims: Vec<Claim> = self.claims.iter().map(|cs| cs.claim.clone()).collect();

        let interim = FinalResult {
            script_id: self.script_id.clone(),
            claims: claims.clone(),
            evidence: self.all_evidence.clone(),
            corrections: self.corrections.clone(),
            final_script: apply_corrections(&self.original_text, &self.corrections),
            verdict: "NEEDS_REVIEW".into(),
            unverified_claim_ids: Vec::new(),
        };

        let validator = Arc::clone(&self.deps.result_validator);
        let outcome = tokio::task::spawn_blocking(move || validator.validate(&interim))
            .await
            .map_err(|e| format!("validator panicked: {e}"))?;

        let final_script = apply_corrections(&self.original_text, &self.corrections);
        let verdict: &str = if outcome.valid && outcome.unverified_claim_ids.is_empty() { "PASS" } else { "NEEDS_REVIEW" };

        info!(script_id = %self.script_id, valid = outcome.valid, unverified = outcome.unverified_claim_ids.len(), "final verification");

        self.runlog(&format!("final verdict={} unverified={}", verdict, outcome.unverified_claim_ids.len()));
        self.final_result = Some(FinalResult {
            script_id: self.script_id.clone(),
            claims,
            evidence: self.all_evidence.clone(),
            corrections: self.corrections.clone(),
            final_script,
            verdict: verdict.into(),
            unverified_claim_ids: outcome.unverified_claim_ids,
        });

        let mut s = self.shared.write().await;
        s.result = self.final_result.clone();
        Ok(())
    }

    async fn do_persist(&mut self) -> Result<(), String> {
        let store = Arc::clone(&self.deps.store);
        let script_id = self.script_id.clone();
        let run_id = self.run_id.clone();
        let claims: Vec<Claim> = self.claims.iter().map(|cs| cs.claim.clone()).collect();
        let evidence = self.all_evidence.clone();
        let corrections = self.corrections.clone();
        let finished = now_iso();

        tokio::task::spawn_blocking(move || {
            for c in &claims { store.save_claim(&script_id, c); }
            for e in &evidence { store.save_evidence(e); }
            for c in &corrections { store.save_correction(c); }
            store.update_run_status(&run_id, "complete", Some(&finished));
        }).await.map_err(|e| format!("persist panicked: {e}"))?;

        Ok(())
    }

    async fn persist_run_start(&self) {
        let store = Arc::clone(&self.deps.store);
        let script_id = self.script_id.clone();
        let text = self.original_text.clone();
        let started = self.started_at.clone();
        let run_id = self.run_id.clone();

        let _ = tokio::task::spawn_blocking(move || {
            store.save_script(&script_id, &text, &started);
            store.save_run(&RunRecord { id: run_id, script_id, status: "running".into(), started_at: started, finished_at: None });
        }).await;
    }

    async fn persist_run_finish(&self) {
        let store = Arc::clone(&self.deps.store);
        let run_id = self.run_id.clone();
        let status: String = match self.stage {
            PipelineStage::Done => "complete",
            PipelineStage::Stopped => "stopped",
            PipelineStage::Failed => "failed",
            _ => "unknown",
        }.into();
        let finished = now_iso();

        let _ = tokio::task::spawn_blocking(move || {
            store.update_run_status(&run_id, &status, Some(&finished));
        }).await;
    }

    async fn set_stage(&mut self, stage: PipelineStage, message: &str) {
        if self.stage == stage && self.message == message { return; }
        info!(script_id = %self.script_id, stage = ?stage, msg = message, "stage transition");
        self.stage = stage;
        self.message = message.to_string();
        self.deps.emitter.emit_progress(stage.as_event_str(), message);
        self.runlog(&format!("stage => {} ({})", stage.as_event_str(), message));

        let mut s = self.shared.write().await;
        s.stage = stage;
        s.message = message.to_string();
    }

    async fn emit_claim_update(&self, idx: usize) {
        let c = self.claims[idx].claim.clone();
        self.deps.emitter.emit_claim_update(&c);

        let mut s = self.shared.write().await;
        s.claims = self.claims.iter().map(|cs| cs.claim.clone()).collect();
    }

    async fn mark_not_running(&self) {
        let mut s = self.shared.write().await;
        s.running = false;
    }
}

async fn run_claim_task(
    deps: Arc<Deps>,
    config: Arc<Config>,
    script_id: String,
    claim: Claim,
    prior_evidence: Vec<Evidence>,
    challenge: Option<ChallengeOutcome>,
    kind: TaskKind,
) -> TaskResult {
    match kind {
        TaskKind::Correction => {
            let engine = Arc::clone(&deps.correction_engine);
            let claim_c = claim.clone();
            let ev = prior_evidence.clone();
            match tokio::task::spawn_blocking(move || engine.correct(&claim_c, &ev)).await {
                Ok(c) => TaskResult::Correction(c),
                Err(e) => TaskResult::Fatal(format!("correction engine panicked: {e}")),
            }
        }
        TaskKind::Research | TaskKind::Challenge | TaskKind::Recheck => {
            run_worker_task(deps, config, script_id, claim, prior_evidence, challenge, kind).await
        }
    }
}

// Audit fix 4c(ii): builds the correct prompt per stage via
// verification_prompts (module 5) instead of an ad-hoc inline prompt, and
// dispatches parsing by `kind` so a challenge response is parsed with
// `parse_challenge` (fix G2), not force-fit through `parse`.
async fn run_worker_task(
    deps: Arc<Deps>,
    config: Arc<Config>,
    script_id: String,
    claim: Claim,
    prior_evidence: Vec<Evidence>,
    challenge: Option<ChallengeOutcome>,
    kind: TaskKind,
) -> TaskResult {
    let provider = match kind {
        TaskKind::Research => config.roles.research.clone(),
        TaskKind::Challenge => config.roles.challenge.clone(),
        TaskKind::Recheck => config.roles.recheck.clone(),
        TaskKind::Correction => unreachable!(),
    };

    let prompt = match kind {
        TaskKind::Research => crate::verification_prompts::build_research_prompt(&claim),
        TaskKind::Challenge => crate::verification_prompts::build_challenge_prompt(&claim, &prior_evidence),
        TaskKind::Recheck => {
            let default_challenge = ChallengeOutcome { agrees: false, reason: String::new(), new_evidence: Vec::new() };
            let c = challenge.as_ref().unwrap_or(&default_challenge);
            crate::verification_prompts::build_recheck_prompt(&claim, &prior_evidence, c)
        }
        TaskKind::Correction => unreachable!(),
    };

    let req = WorkerRequest {
        req_id: next_id("req"),
        provider: provider.clone(),
        action: "send_prompt".into(),
        prompt,
        page_id: format!("{provider}-{}", claim.id),
    };

    // Audit fix 4d: retries is now signed in config.rs (so negative values
    // fail validation instead of a silent serde parse error); use the
    // clamped unsigned accessor here.
    let max_retries = config.retries_u32();
    let mut attempt: u32 = 0;

    loop {
        let bw = Arc::clone(&deps.browser_worker);
        let req_clone = req.clone();
        let resp = match tokio::task::spawn_blocking(move || bw.send(req_clone)).await {
            Ok(r) => r,
            Err(e) => return TaskResult::Fatal(format!("browser worker panicked: {e}")),
        };

        match resp {
            WorkerResponse::Ok { text, .. } => match kind {
                TaskKind::Challenge => {
                    let parser = Arc::clone(&deps.evidence_parser);
                    let cid = claim.id.clone();
                    let outcome = match tokio::task::spawn_blocking(move || parser.parse_challenge(&text, &cid)).await {
                        Ok(v) => v,
                        Err(e) => return TaskResult::Fatal(format!("challenge parser panicked: {e}")),
                    };
                    return TaskResult::Challenge(outcome);
                }
                _ => {
                    let parser = Arc::clone(&deps.evidence_parser);
                    let cid = claim.id.clone();
                    let parsed = match tokio::task::spawn_blocking(move || parser.parse(&text, &cid)).await {
                        Ok(v) => v,
                        Err(e) => return TaskResult::Fatal(format!("evidence parser panicked: {e}")),
                    };
                    return TaskResult::Evidence(parsed);
                }
            },
            WorkerResponse::Error { message, .. } => {
                if attempt >= max_retries {
                    return TaskResult::Fatal(format!(
                        "browser worker failed after {} attempts: {}",
                        attempt + 1,
                        message
                    ));
                }
                let backoff_ms: u64 = 200u64.saturating_mul(1u64 << attempt.min(10));
                attempt += 1;
                warn!(script_id = %script_id, claim = %claim.id, attempt, backoff_ms, "worker error, retrying after backoff: {message}");
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            }
        }
    }
}

fn apply_corrections(original: &str, corrections: &[Correction]) -> String {
    let mut out = original.to_string();
    for c in corrections {
        if c.original_text.is_empty() { continue; }
        out = out.replacen(&c.original_text, &c.corrected_text, 1);
    }
    out
}

static SEQ: AtomicU64 = AtomicU64::new(0);

pub fn next_id(prefix: &str) -> String {
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0);
    format!("{prefix}-{t:x}{n:04x}")
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}


/// Returns the next phase a task of `kind` will move the claim to. Purely
/// informational — used only for the per-run log.
fn next_phase_hint(kind: TaskKind) -> &'static str {
    match kind {
        TaskKind::Research => "AwaitingChallenge",
        TaskKind::Challenge => "AwaitingCorrection",
        TaskKind::Recheck => "AwaitingCorrection",
        TaskKind::Correction => "Complete",
    }
}
