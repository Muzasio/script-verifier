//! LLM-backed `ScriptProcessor`.

use std::collections::HashSet;
use std::sync::Arc;

use serde::Deserialize;
use tracing::{debug, error, info, warn};

use crate::models::{Claim, WorkerRequest, WorkerResponse};
use crate::orchestrator::next_id;
use crate::traits::{BrowserWorker, ScriptProcessor};

const EXTRACTION_PROMPT: &str = "You will be given a technical script. \
Extract every atomic, independently verifiable technical claim (a factual \
assertion about how something works, a specific number, a named \
standard/protocol/API behavior, a version-specific fact). Do not extract \
opinions, style commentary, or claims that are not independently \
checkable. For each claim return JSON only, an array of objects: {id, \
text, original_wording, context}. id is a sequential string 'c-0001', \
'c-0002', ... . original_wording is the exact verbatim substring from the \
input. context is the surrounding 1-2 sentences. Output nothing but the \
JSON array.\n\nSCRIPT:\n";

const RETRY_INSTRUCTION: &str = "\n\nYour previous response was not valid JSON. \
Return only the JSON array, nothing else.";

pub struct LlmScriptProcessor {
    worker: Arc<dyn BrowserWorker>,
    provider: String,
}

impl LlmScriptProcessor {
    pub fn new(worker: Arc<dyn BrowserWorker>, provider: impl Into<String>) -> Self {
        Self { worker, provider: provider.into() }
    }

    fn call_and_parse(&self, prompt: &str) -> Attempt {
        let req = WorkerRequest {
            req_id: next_id("req"),
            provider: self.provider.clone(),
            action: "send_prompt".into(),
            prompt: prompt.to_string(),
            page_id: format!("{}-extract", self.provider),
        };

        match self.worker.send(req) {
            WorkerResponse::Ok { text, .. } => match parse_claims(&text) {
                Some(claims) => Attempt::Parsed(claims),
                None => {
                    debug!(raw_len = text.len(), "response was not valid JSON");
                    Attempt::Unparseable { raw: text }
                }
            },
            WorkerResponse::Error { message, .. } => {
                warn!(error = %message, "browser worker returned error");
                Attempt::Unparseable { raw: format!("[worker error] {message}") }
            }
        }
    }
}

impl ScriptProcessor for LlmScriptProcessor {
    fn extract_claims(&self, text: &str) -> Vec<Claim> {
        let prompt = format!("{EXTRACTION_PROMPT}{text}");

        match self.call_and_parse(&prompt) {
            Attempt::Parsed(claims) => {
                info!(n = claims.len(), "claims extracted");
                return claims;
            }
            Attempt::Unparseable { raw } => {
                warn!(raw_len = raw.len(), "extraction attempt 1 was unparseable; retrying once");
            }
        }

        let retry_prompt = format!("{prompt}{RETRY_INSTRUCTION}");
        match self.call_and_parse(&retry_prompt) {
            Attempt::Parsed(claims) => {
                info!(n = claims.len(), "claims extracted after retry");
                claims
            }
            Attempt::Unparseable { raw } => {
                error!(raw_response = %raw, "extraction failed twice; returning no claims");
                Vec::new()
            }
        }
    }
}

enum Attempt {
    Parsed(Vec<Claim>),
    Unparseable { raw: String },
}

#[derive(Deserialize)]
struct RawClaim {
    #[serde(default)] id: String,
    #[serde(default)] text: String,
    #[serde(default)] original_wording: String,
    #[serde(default)] context: String,
}

fn parse_claims(raw: &str) -> Option<Vec<Claim>> {
    let cleaned = strip_code_fences(raw);
    let parsed: Vec<RawClaim> = serde_json::from_str(&cleaned).ok()?;

    let mut seen: HashSet<String> = HashSet::new();
    let mut deduped: Vec<RawClaim> = Vec::with_capacity(parsed.len());
    for rc in parsed {
        if rc.original_wording.trim().is_empty() {
            continue;
        }
        if !seen.insert(rc.original_wording.clone()) {
            debug!(original_wording = %rc.original_wording, "dropped duplicate claim");
            continue;
        }
        deduped.push(rc);
    }

    let claims = deduped
        .into_iter()
        .enumerate()
        .map(|(i, rc)| {
            let id = if rc.id.trim().is_empty() { format!("c-{:04}", i + 1) } else { rc.id };
            Claim {
                id,
                text: rc.text,
                original_wording: rc.original_wording,
                context: rc.context,
                status: "pending".into(),
            }
        })
        .collect();

    Some(claims)
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
