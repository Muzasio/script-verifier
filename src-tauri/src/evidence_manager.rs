//! Evidence deduplication, quality ranking, and conflict resolution.

use std::collections::{HashMap, HashSet};

use crate::models::Evidence;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    Supports,
    Contradicts,
    Unresolved,
}

#[derive(Debug, Clone)]
pub struct Conflict {
    pub claim_id: String,
    pub supporting: Vec<Evidence>,
    pub contradicting: Vec<Evidence>,
}

pub const DEFAULT_SOURCE_PRIORITY: &[&str] = &[
    "rfc_standard",
    "official_docs",
    "paper",
    "reputable_technical",
    "other",
];

pub fn dedupe_evidence(evidence: Vec<Evidence>) -> Vec<Evidence> {
    let mut index: HashMap<(String, String), usize> = HashMap::new();
    let mut out: Vec<Evidence> = Vec::with_capacity(evidence.len());

    for e in evidence {
        if e.source_url.trim().is_empty() {
            out.push(e);
            continue;
        }
        let key = (e.claim_id.clone(), e.source_url.clone());
        match index.get(&key).copied() {
            Some(i) => merge_into(&mut out[i], e),
            None => {
                index.insert(key, out.len());
                out.push(e);
            }
        }
    }
    out
}

fn merge_into(dst: &mut Evidence, src: Evidence) {
    let dst_rank = tier_rank(&dst.source_tier, DEFAULT_SOURCE_PRIORITY);
    let src_rank = tier_rank(&src.source_tier, DEFAULT_SOURCE_PRIORITY);
    let merged_excerpt = merge_excerpts(&dst.excerpt, &src.excerpt);

    if src_rank < dst_rank {
        let kept_id = dst.id.clone();
        let mut winner = src;
        if winner.id.is_none() {
            winner.id = kept_id;
        }
        winner.excerpt = merged_excerpt;
        *dst = winner;
    } else {
        dst.excerpt = merged_excerpt;
    }
}

fn merge_excerpts(a: &str, b: &str) -> String {
    let a = a.trim();
    let b = b.trim();
    if a == b { return a.to_string(); }
    if a.is_empty() { return b.to_string(); }
    if b.is_empty() { return a.to_string(); }
    const SEPARATOR: &str = " | ";
    if a.len() + b.len() + SEPARATOR.len() < 300 {
        format!("{a}{SEPARATOR}{b}")
    } else {
        a.to_string()
    }
}

pub fn rank_by_tier(evidence: &mut Vec<Evidence>, priority: &[String]) {
    evidence.sort_by_key(|e| tier_rank(&e.source_tier, priority));
}

pub fn detect_conflicts(evidence: &[Evidence]) -> Vec<Conflict> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<&Evidence>> = HashMap::new();

    for e in evidence {
        if !groups.contains_key(&e.claim_id) {
            order.push(e.claim_id.clone());
        }
        groups.entry(e.claim_id.clone()).or_default().push(e);
    }

    let mut out = Vec::new();
    for cid in order {
        let group = &groups[&cid];
        let supporting: Vec<Evidence> = group.iter().filter(|e| e.verdict == "supports").map(|e| (*e).clone()).collect();
        let contradicting: Vec<Evidence> = group.iter().filter(|e| e.verdict == "contradicts").map(|e| (*e).clone()).collect();
        if !supporting.is_empty() && !contradicting.is_empty() {
            out.push(Conflict { claim_id: cid, supporting, contradicting });
        }
    }
    out
}

pub fn resolve_conflict(conflict: &Conflict) -> Resolution {
    let sup_tier = best_tier_rank(&conflict.supporting);
    let con_tier = best_tier_rank(&conflict.contradicting);

    if sup_tier < con_tier { return Resolution::Supports; }
    if con_tier < sup_tier { return Resolution::Contradicts; }

    let sup_domains = unique_domain_count(&conflict.supporting);
    let con_domains = unique_domain_count(&conflict.contradicting);

    if sup_domains > con_domains { Resolution::Supports }
    else if con_domains > sup_domains { Resolution::Contradicts }
    else { Resolution::Unresolved }
}

fn best_tier_rank(evidence: &[Evidence]) -> usize {
    evidence.iter().map(|e| tier_rank(&e.source_tier, DEFAULT_SOURCE_PRIORITY)).min().unwrap_or(usize::MAX)
}

fn unique_domain_count(evidence: &[Evidence]) -> usize {
    let mut domains: HashSet<String> = HashSet::new();
    for e in evidence {
        let d = domain_of(&e.source_url);
        if !d.is_empty() { domains.insert(d); }
    }
    domains.len()
}

fn domain_of(url: &str) -> String {
    let after_scheme = url.splitn(2, "://").nth(1).unwrap_or(url);
    let host = after_scheme.split(|c: char| c == '/' || c == '?' || c == '#').next().unwrap_or("");
    let host = host.split('@').next_back().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    host.trim().to_lowercase()
}

fn tier_rank<S: AsRef<str>>(tier: &str, priority: &[S]) -> usize {
    priority.iter().position(|p| p.as_ref() == tier).unwrap_or(priority.len())
}
