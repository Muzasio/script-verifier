//! Prompt templates + response parsers for the verification pipeline.

use serde::Deserialize;
use tracing::warn;

use crate::models::{Claim, Correction, Evidence};

// Audit fix 4a: ChallengeOutcome now lives in models.rs so traits.rs can
// reference it without importing this module. Re-exported here so existing
// call sites (`verification_prompts::ChallengeOutcome`) still resolve.
pub use crate::models::ChallengeOutcome;

pub const JSON_ONLY_INSTRUCTION: &str =
    "Output valid JSON only, no markdown formatting, no commentary before or after the JSON.";

const PROVIDER_UNSPECIFIED: &str = "";

pub fn build_research_prompt(claim: &Claim) -> String {
    format!(
        "You are a technical fact-checker. Research the claim below and \
report whether credible sources support or contradict it.\n\
\n\
Claim:\n\
{text}\n\
\n\
Original wording from the script:\n\
\"{orig}\"\n\
\n\
Surrounding context:\n\
{ctx}\n\
\n\
Source tier priority — use the highest tier you can find:\n\
  1. rfc_standard       — RFCs, W3C specs, ISO/IEC, IETF standards\n\
  2. official_docs      — vendor/project documentation (docs.rs, MDN, etc.)\n\
  3. paper              — peer-reviewed or arXiv-grade technical papers\n\
  4. reputable_technical— maintainer blogs, well-known engineering pubs\n\
  5. other              — personal blogs, forums, Q&A sites. Use ONLY when\n\
                          no higher-tier source exists, and mark the source\n\
                          with source_tier \"other\".\n\
\n\
Do not invent URLs. Do not cite a source you cannot verify. If no credible \
source can be found, set verdict to \"insufficient\" with an empty sources \
array.\n\
\n\
Return a JSON object with this shape:\n\
{{\n\
  \"verdict\": \"supports\" | \"contradicts\" | \"insufficient\",\n\
  \"sources\": [\n\
    {{ \"url\": \"...\", \"title\": \"...\", \"excerpt\": \"...\", \
\"source_tier\": \"rfc_standard\" | \"official_docs\" | \"paper\" | \
\"reputable_technical\" | \"other\" }}\n\
  ]\n\
}}\n\
\n\
{JSON_ONLY_INSTRUCTION}",
        text = claim.text,
        orig = claim.original_wording,
        ctx = claim.context,
    )
}

pub fn parse_research_response(raw: &str, claim_id: &str) -> Vec<Evidence> {
    let cleaned = strip_code_fences(raw);
    let parsed: RawResearch = match serde_json::from_str(&cleaned) {
        Ok(v) => v,
        Err(e) => {
            warn!(claim_id = %claim_id, error = %e, raw_response = %raw, "research response was not valid JSON; returning no evidence");
            return Vec::new();
        }
    };

    let verdict = normalize_verdict(&parsed.verdict);
    parsed
        .sources
        .into_iter()
        .filter_map(|s| raw_source_to_evidence(s, claim_id, &verdict))
        .collect()
}

pub fn build_challenge_prompt(claim: &Claim, evidence: &[Evidence]) -> String {
    let mut findings = String::new();
    if evidence.is_empty() {
        findings.push_str("  (no prior evidence was produced)\n");
    } else {
        for (i, e) in evidence.iter().enumerate() {
            findings.push_str(&format!(
                "  {n}. [{verdict}] {title} — {url}\n     excerpt: {excerpt}\n     tier: {tier}\n",
                n = i + 1,
                verdict = e.verdict,
                title = if e.source_title.is_empty() { "(untitled)" } else { &e.source_title },
                url = e.source_url,
                excerpt = e.excerpt,
                tier = e.source_tier,
            ));
        }
    }

    format!(
        "You are a skeptical second reviewer. Another provider has already \
researched the claim below. Your job is NOT to rubber-stamp their \
findings: actively look for reasons the prior verdict or its sources \
might be wrong, outdated, misread, or insufficient.\n\
\n\
Claim:\n\
{text}\n\
\n\
Original wording:\n\
\"{orig}\"\n\
\n\
Prior findings ({n} source(s)):\n\
{findings}\
\n\
Specifically check:\n\
  - Is the verdict actually justified by the cited sources, or does it\n\
    overreach what they say?\n\
  - Are the sources authoritative for THIS claim, or merely adjacent?\n\
  - Are the sources current, or has the fact changed since publication?\n\
  - Is a key qualifier, edge case, or version boundary being ignored?\n\
  - Would a different reasonable reading of the sources flip the verdict?\n\
\n\
Return a JSON object with this shape:\n\
{{\n\
  \"agrees\": true | false,\n\
  \"reason\": \"concise justification for agree/disagree\",\n\
  \"new_sources\": [\n\
    {{ \"url\": \"...\", \"title\": \"...\", \"excerpt\": \"...\", \
\"source_tier\": \"rfc_standard\" | \"official_docs\" | \"paper\" | \
\"reputable_technical\" | \"other\" }}\n\
  ]\n\
}}\n\
new_sources is OPTIONAL — include it only if you found sources the prior \
pass missed. Omit the key entirely otherwise.\n\
\n\
{JSON_ONLY_INSTRUCTION}",
        text = claim.text,
        orig = claim.original_wording,
        n = evidence.len(),
        findings = findings,
    )
}

pub fn parse_challenge_response(raw: &str) -> ChallengeOutcome {
    let cleaned = strip_code_fences(raw);
    let parsed: RawChallenge = match serde_json::from_str(&cleaned) {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, raw_response = %raw, "challenge response was not valid JSON; defaulting to agrees=true");
            return ChallengeOutcome { agrees: true, reason: String::new(), new_evidence: Vec::new() };
        }
    };

    let new_evidence = parsed
        .new_sources
        .into_iter()
        .filter_map(|s| raw_source_to_evidence(s, "", "insufficient"))
        .collect();

    ChallengeOutcome { agrees: parsed.agrees, reason: parsed.reason, new_evidence }
}

pub fn build_recheck_prompt(claim: &Claim, evidence: &[Evidence], challenge: &ChallengeOutcome) -> String {
    let mut prior = String::new();
    if evidence.is_empty() {
        prior.push_str("  (none)\n");
    } else {
        for (i, e) in evidence.iter().enumerate() {
            prior.push_str(&format!(
                "  {n}. [{verdict}] {title} — {url}\n     excerpt: {excerpt}\n",
                n = i + 1,
                verdict = e.verdict,
                title = if e.source_title.is_empty() { "(untitled)" } else { &e.source_title },
                url = e.source_url,
                excerpt = e.excerpt,
            ));
        }
    }

    let mut extra = String::new();
    if challenge.new_evidence.is_empty() {
        extra.push_str("  (none)\n");
    } else {
        for (i, e) in challenge.new_evidence.iter().enumerate() {
            extra.push_str(&format!(
                "  {n}. {title} — {url}\n     excerpt: {excerpt}\n",
                n = i + 1,
                title = if e.source_title.is_empty() { "(untitled)" } else { &e.source_title },
                url = e.source_url,
                excerpt = e.excerpt,
            ));
        }
    }

    format!(
        "Your original verdict on the claim below has been challenged. \
Re-verify the claim, taking the objection seriously. You may keep your \
previous verdict or change it — but the final verdict must be justified \
against the objection.\n\
\n\
Claim:\n\
{text}\n\
\n\
Original wording:\n\
\"{orig}\"\n\
\n\
Your prior evidence ({n} source(s)):\n\
{prior}\
\n\
Challenge objection:\n\
{reason}\n\
\n\
Sources the challenger added ({m}):\n\
{extra}\
\n\
Return a JSON object with this shape:\n\
{{\n\
  \"verdict\": \"supports\" | \"contradicts\" | \"insufficient\",\n\
  \"sources\": [\n\
    {{ \"url\": \"...\", \"title\": \"...\", \"excerpt\": \"...\", \
\"source_tier\": \"rfc_standard\" | \"official_docs\" | \"paper\" | \
\"reputable_technical\" | \"other\" }}\n\
  ]\n\
}}\n\
\n\
{JSON_ONLY_INSTRUCTION}",
        text = claim.text,
        orig = claim.original_wording,
        n = evidence.len(),
        prior = prior,
        reason = challenge.reason,
        m = challenge.new_evidence.len(),
        extra = extra,
    )
}

pub fn build_correction_prompt(claim: &Claim, evidence: &[Evidence]) -> String {
    let mut ev_block = String::new();
    if evidence.is_empty() {
        ev_block.push_str("  (no evidence — the claim is unsupported)\n");
    } else {
        for (i, e) in evidence.iter().enumerate() {
            ev_block.push_str(&format!(
                "  {n}. [{verdict}] {title} — {url}\n     excerpt: {excerpt}\n     tier: {tier}\n",
                n = i + 1,
                verdict = e.verdict,
                title = if e.source_title.is_empty() { "(untitled)" } else { &e.source_title },
                url = e.source_url,
                excerpt = e.excerpt,
                tier = e.source_tier,
            ));
        }
    }

    format!(
        "You are a technical editor. A claim in the user's script is \
factually wrong or unsupported by the evidence below. Rewrite ONLY the \
claim's original wording so it is technically accurate given the \
evidence, while obeying every constraint:\n\
\n\
  - Preserve the author's tone, register, and style.\n\
  - Preserve the claim's role in the surrounding text: do not remove it,\n\
    do not expand its scope, do not turn it into a tutorial.\n\
  - Keep the corrected wording close in length to the original unless\n\
    accuracy strictly requires otherwise.\n\
  - Do not add hedging language unless the evidence genuinely supports\n\
    only a qualified statement.\n\
  - Do not add new facts that are not in the evidence.\n\
\n\
Claim (as written):\n\
\"{orig}\"\n\
\n\
Surrounding context:\n\
{ctx}\n\
\n\
Evidence:\n\
{ev}\
\n\
Return a JSON object with this shape:\n\
{{\n\
  \"corrected_text\": \"...\",\n\
  \"reason\": \"short explanation of what was wrong and what changed\"\n\
}}\n\
\n\
{JSON_ONLY_INSTRUCTION}",
        orig = claim.original_wording,
        ctx = claim.context,
        ev = ev_block,
    )
}

pub fn parse_correction_response(raw: &str, claim: &Claim, evidence_ids: Vec<String>) -> Option<Correction> {
    let cleaned = strip_code_fences(raw);
    let parsed: RawCorrection = match serde_json::from_str(&cleaned) {
        Ok(v) => v,
        Err(e) => {
            warn!(claim_id = %claim.id, error = %e, raw_response = %raw, "correction response was not valid JSON; returning None");
            return None;
        }
    };

    let corrected = parsed.corrected_text.trim();
    if corrected.is_empty() {
        warn!(claim_id = %claim.id, "correction response had empty corrected_text");
        return None;
    }
    if corrected == claim.original_wording.trim() {
        return None;
    }

    Some(Correction {
        claim_id: claim.id.clone(),
        original_text: claim.original_wording.clone(),
        corrected_text: corrected.to_string(),
        reason: parsed.reason,
        evidence_ids,
    })
}

#[derive(Deserialize)]
struct RawResearch {
    #[serde(default)] verdict: String,
    #[serde(default)] sources: Vec<RawSource>,
}

#[derive(Deserialize)]
struct RawChallenge {
    #[serde(default)] agrees: bool,
    #[serde(default)] reason: String,
    #[serde(default)] new_sources: Vec<RawSource>,
}

#[derive(Deserialize)]
struct RawCorrection {
    #[serde(default)] corrected_text: String,
    #[serde(default)] reason: String,
}

#[derive(Deserialize)]
struct RawSource {
    #[serde(default)] url: String,
    #[serde(default)] title: String,
    #[serde(default)] excerpt: String,
    #[serde(default)] source_tier: Option<String>,
}

fn raw_source_to_evidence(s: RawSource, claim_id: &str, verdict: &str) -> Option<Evidence> {
    if s.url.trim().is_empty() && s.title.trim().is_empty() {
        return None;
    }
    let tier = s.source_tier.as_deref().map(normalize_tier).unwrap_or_else(|| "other".to_string());
    Some(Evidence {
        id: None,
        claim_id: claim_id.to_string(),
        source_url: s.url,
        source_title: s.title,
        excerpt: s.excerpt,
        source_tier: tier,
        verdict: verdict.to_string(),
        provider: PROVIDER_UNSPECIFIED.to_string(),
    })
}

fn normalize_tier(raw: &str) -> String {
    let key = raw.trim().to_lowercase();
    let key = key.replace([' ', '-'], "_");
    match key.as_str() {
        "rfc_standard" | "rfc" | "standard" | "rfc_standard_document" => "rfc_standard",
        "official_docs" | "official" | "docs" | "official_documentation" => "official_docs",
        "paper" | "peer_reviewed" | "academic" | "arxiv" => "paper",
        "reputable_technical" | "reputable" | "technical" => "reputable_technical",
        _ => "other",
    }
    .to_string()
}

fn normalize_verdict(raw: &str) -> String {
    match raw.trim().to_lowercase().as_str() {
        "supports" => "supports",
        "contradicts" => "contradicts",
        _ => "insufficient",
    }
    .to_string()
}

/// Extracts the first balanced JSON value (array or object) from an LLM
/// response. Handles three common LLM behaviours:
///   1. Raw JSON as the entire response.
///   2. JSON wrapped in a markdown code fence (```json ... ```).
///   3. JSON preceded by preamble prose ("Gemini said\n\n[...]").
///
/// Tracks string literals and escapes so brackets inside strings do not
/// break the balance scan. Returns the input trimmed if no JSON is found;
/// the caller's `serde_json::from_str` will then fail as before.
fn strip_code_fences(s: &str) -> String {
    let trimmed = s.trim();

    if trimmed.starts_with('[') || trimmed.starts_with('{') {
        return trimmed.to_string();
    }

    if trimmed.starts_with("```") {
        let body = match trimmed.find('\n') {
            Some(i) => &trimmed[i + 1..],
            None => return trimmed.trim_matches('`').trim().to_string(),
        };
        let body = match body.rfind("```") {
            Some(i) => &body[..i],
            None => body,
        };
        return extract_first_json(body.trim()).unwrap_or_else(|| body.trim().to_string());
    }

    extract_first_json(trimmed).unwrap_or_else(|| trimmed.to_string())
}

/// Returns the first balanced `[...]` or `{...}` slice in `s`, respecting
/// string literals and `\` escapes inside them.
fn extract_first_json(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let start = bytes.iter().position(|&b| b == b'[' || b == b'{')?;

    let open = bytes[start];
    let close = if open == b'[' { b']' } else { b'}' };

    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escaped = false;

    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            x if x == open => depth += 1,
            x if x == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[start..=i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}
