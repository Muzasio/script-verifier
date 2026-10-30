//! Final-pass integrity checks and final-script assembly.

use std::collections::{BTreeSet, HashMap};

use tracing::warn;

use crate::models::{Claim, Correction, Evidence, FinalResult};

pub const VERDICT_PASS: &str = "PASS";
pub const VERDICT_NEEDS_REVIEW: &str = "NEEDS_REVIEW";

#[derive(Debug, Clone)]
pub struct ValidationFailure {
    pub claim_id: String,
    pub rule: String,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct ValidationOutcome {
    pub verdict: String,
    pub failures: Vec<ValidationFailure>,
}

impl ValidationOutcome {
    pub fn is_pass(&self) -> bool {
        self.verdict == VERDICT_PASS
    }
}

pub mod rules {
    pub const VERIFIED_REQUIRES_SUPPORT: &str = "verified_requires_supporting_evidence";
    pub const CORRECTED_REQUIRES_CORRECTION: &str = "corrected_requires_correction";
    pub const CORRECTION_EVIDENCE_UNRESOLVED: &str = "correction_evidence_ids_unresolved";
    pub const UNRESOLVED_STATUS: &str = "unresolved_claim_status";
    pub const EVIDENCE_URL_INVALID: &str = "evidence_url_invalid";
    pub const UNVERIFIED_IDS_MISSING: &str = "unverified_claim_ids_missing";
    pub const UNVERIFIED_IDS_EXTRA: &str = "unverified_claim_ids_extra";
    pub const UNVERIFIED_IDS_DUPLICATE: &str = "unverified_claim_ids_duplicate";
    pub const NO_CLAIMS_EXTRACTED: &str = "no_claims_extracted";
}

pub fn validate(result: &FinalResult) -> ValidationOutcome {
    let mut failures: Vec<ValidationFailure> = Vec::new();
    // A result with no claims means extraction failed silently; never PASS it.
    if result.claims.is_empty() {
        failures.push(ValidationFailure {
            claim_id: String::new(),
            rule: rules::NO_CLAIMS_EXTRACTED.into(),
            detail: "no claims were extracted from the script — the extraction \
                     step likely failed to parse the LLM response"
                .into(),
        });
    }

    let evidence_by_id = index_evidence_by_id(result);
    let corrections_by_claim = index_corrections_by_claim(result);

    for claim in &result.claims {
        if claim.status != "verified" { continue; }
        let has_support = result.evidence.iter().any(|e| e.claim_id == claim.id && e.verdict == "supports");
        if !has_support {
            failures.push(ValidationFailure {
                claim_id: claim.id.clone(),
                rule: rules::VERIFIED_REQUIRES_SUPPORT.into(),
                detail: "claim is marked verified but has no Evidence with verdict \"supports\"".into(),
            });
        }
    }

    for claim in &result.claims {
        if claim.status != "corrected" { continue; }
        let corrections = corrections_by_claim.get(claim.id.as_str()).map(Vec::as_slice).unwrap_or(&[]);

        if corrections.is_empty() {
            failures.push(ValidationFailure {
                claim_id: claim.id.clone(),
                rule: rules::CORRECTED_REQUIRES_CORRECTION.into(),
                detail: "claim is marked corrected but no Correction entry exists for it".into(),
            });
            continue;
        }

        for corr in corrections {
            for ev_id in &corr.evidence_ids {
                match evidence_by_id.get(ev_id.as_str()) {
                    Some(e) if e.claim_id == claim.id => {}
                    Some(other) => {
                        failures.push(ValidationFailure {
                            claim_id: claim.id.clone(),
                            rule: rules::CORRECTION_EVIDENCE_UNRESOLVED.into(),
                            detail: format!("correction references evidence {ev_id} which belongs to claim {}", other.claim_id),
                        });
                    }
                    None => {
                        failures.push(ValidationFailure {
                            claim_id: claim.id.clone(),
                            rule: rules::CORRECTION_EVIDENCE_UNRESOLVED.into(),
                            detail: format!("correction references evidence id {ev_id} that does not exist"),
                        });
                    }
                }
            }
        }
    }

    for claim in &result.claims {
        if claim.status == "pending" || claim.status == "researching" {
            failures.push(ValidationFailure {
                claim_id: claim.id.clone(),
                rule: rules::UNRESOLVED_STATUS.into(),
                detail: format!("claim still has status \"{}\" at validation time", claim.status),
            });
        }
    }

    for e in &result.evidence {
        let url = e.source_url.trim();
        let id_label = e.id.as_deref().unwrap_or("<no id>");
        if url.is_empty() {
            failures.push(ValidationFailure { claim_id: e.claim_id.clone(), rule: rules::EVIDENCE_URL_INVALID.into(), detail: format!("evidence {id_label} has an empty source_url") });
            continue;
        }
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            failures.push(ValidationFailure { claim_id: e.claim_id.clone(), rule: rules::EVIDENCE_URL_INVALID.into(), detail: format!("evidence {id_label} source_url does not start with http:// or https://: {url}") });
        }
    }

    let expected: BTreeSet<String> = result.claims.iter().filter(|c| c.status == "unverified").map(|c| c.id.clone()).collect();
    let actual: BTreeSet<String> = result.unverified_claim_ids.iter().cloned().collect();

    for id in expected.difference(&actual) {
        failures.push(ValidationFailure { claim_id: id.clone(), rule: rules::UNVERIFIED_IDS_MISSING.into(), detail: format!("claim {id} has status \"unverified\" but is missing from unverified_claim_ids") });
    }
    for id in actual.difference(&expected) {
        failures.push(ValidationFailure { claim_id: id.clone(), rule: rules::UNVERIFIED_IDS_EXTRA.into(), detail: format!("unverified_claim_ids contains {id} but no claim with that id has status \"unverified\"") });
    }
    if result.unverified_claim_ids.len() != actual.len() {
        failures.push(ValidationFailure { claim_id: String::new(), rule: rules::UNVERIFIED_IDS_DUPLICATE.into(), detail: format!("unverified_claim_ids has {} entries but only {} unique", result.unverified_claim_ids.len(), actual.len()) });
    }

    if failures.is_empty() {
        ValidationOutcome { verdict: VERDICT_PASS.into(), failures }
    } else {
        ValidationOutcome { verdict: VERDICT_NEEDS_REVIEW.into(), failures }
    }
}

pub fn assemble_final_result(script_id: &str, original_script: &str, claims: Vec<Claim>, evidence: Vec<Evidence>, corrections: Vec<Correction>) -> FinalResult {
    let mut final_script = original_script.to_string();

    for corr in &corrections {
        if corr.original_text.is_empty() {
            warn!(claim_id = %corr.claim_id, "correction has empty original_text; skipping");
            continue;
        }
        if final_script.contains(&corr.original_text) {
            final_script = final_script.replacen(&corr.original_text, &corr.corrected_text, 1);
        } else {
            warn!(claim_id = %corr.claim_id, original_text = %corr.original_text, "correction original_text not found verbatim in script; leaving script unchanged for this correction");
        }
    }

    let unverified_claim_ids: Vec<String> = claims.iter().filter(|c| c.status == "unverified").map(|c| c.id.clone()).collect();

    FinalResult {
        script_id: script_id.to_string(),
        claims,
        evidence,
        corrections,
        final_script,
        verdict: VERDICT_NEEDS_REVIEW.into(),
        unverified_claim_ids,
    }
}

pub struct RuleValidator;

impl crate::traits::ResultValidator for RuleValidator {
    fn validate(&self, result: &FinalResult) -> crate::traits::ValidationOutcome {
        let outcome = validate(result);

        let unverified_claim_ids: Vec<String> = result.claims.iter().filter(|c| c.status == "unverified").map(|c| c.id.clone()).collect();

        let notes: Vec<String> = outcome.failures.iter().map(|f| {
            let who = if f.claim_id.is_empty() { "global" } else { f.claim_id.as_str() };
            format!("[{who}] {} — {}", f.rule, f.detail)
        }).collect();

        crate::traits::ValidationOutcome {
            valid: outcome.is_pass(),
            unverified_claim_ids,
            notes,
        }
    }
}

fn index_evidence_by_id(result: &FinalResult) -> HashMap<&str, &Evidence> {
    let mut map = HashMap::with_capacity(result.evidence.len());
    for e in &result.evidence {
        if let Some(id) = e.id.as_deref() {
            if !id.is_empty() { map.insert(id, e); }
        }
    }
    map
}

fn index_corrections_by_claim(result: &FinalResult) -> HashMap<&str, Vec<&Correction>> {
    let mut map: HashMap<&str, Vec<&Correction>> = HashMap::new();
    for c in &result.corrections {
        map.entry(c.claim_id.as_str()).or_default().push(c);
    }
    map
}
