export type Provider = "claude" | "gemini" | "deepseek";

export type ClaimStatus =
  | "pending"
  | "researching"
  | "verified"
  | "contradicted"
  | "corrected"
  | "unverified";

export interface Claim {
  id: string;
  text: string;
  original_wording: string;
  context: string;
  status: ClaimStatus;
}

export type SourceTier =
  | "rfc_standard"
  | "official_docs"
  | "paper"
  | "reputable_technical"
  | "other";

export type EvidenceVerdict = "supports" | "contradicts" | "insufficient";

export interface Evidence {
  id?: string;
  claim_id: string;
  source_url: string;
  source_title: string;
  excerpt: string;
  source_tier: SourceTier;
  verdict: EvidenceVerdict;
  provider: Provider;
}

export interface Correction {
  claim_id: string;
  original_text: string;
  corrected_text: string;
  reason: string;
  evidence_ids: string[];
}

export type FinalVerdict = "PASS" | "NEEDS_REVIEW";

export interface FinalResult {
  script_id: string;
  claims: Claim[];
  evidence: Evidence[];
  corrections: Correction[];
  final_script: string;
  verdict: FinalVerdict;
  unverified_claim_ids: string[];
}

export type Stage =
  | "idle"
  | "extracting_claims"
  | "researching"
  | "challenging"
  | "rechecking"
  | "correcting"
  | "final_verification"
  | "done"
  | "stopped"
  | "error";

export interface ProgressState {
  script_id: string;
  stage: Stage;
  message: string;
  running: boolean;
}

export interface ProgressEvent {
  stage: Stage;
  message: string;
}

export const PIPELINE_STAGES: ReadonlyArray<{ id: Stage; label: string }> = [
  { id: "extracting_claims", label: "Extracting claims" },
  { id: "researching", label: "Researching" },
  { id: "challenging", label: "Challenging" },
  { id: "rechecking", label: "Rechecking" },
  { id: "correcting", label: "Correcting" },
  { id: "final_verification", label: "Final verification" },
];

const STAGE_ALIASES: Record<string, Stage> = {
  idle: "idle",
  extracting_claims: "extracting_claims",
  extracting: "extracting_claims",
  extract_claims: "extracting_claims",
  research: "researching",
  researching: "researching",
  challenge: "challenging",
  challenging: "challenging",
  recheck: "rechecking",
  rechecking: "rechecking",
  correct: "correcting",
  correcting: "correcting",
  correction: "correcting",
  final: "final_verification",
  final_verification: "final_verification",
  done: "done",
  complete: "done",
  completed: "done",
  stopped: "stopped",
  cancelled: "stopped",
  error: "error",
  failed: "error",
};

export function normalizeStage(raw: string): Stage {
  const key = String(raw ?? "")
    .trim()
    .toLowerCase()
    .replace(/[\s-]+/g, "_");
  return STAGE_ALIASES[key] ?? (key as Stage);
}
