import type { FinalResult } from "./types";

export function resultToMarkdown(r: FinalResult): string {
  const out: string[] = [];

  out.push(`# Verification Result — ${r.script_id}`, "");
  out.push(`**Verdict:** ${r.verdict}`, "");

  if (r.unverified_claim_ids.length > 0) {
    out.push("**Unverified claims:** " + r.unverified_claim_ids.join(", "), "");
  }

  out.push("## Final script", "", "```", r.final_script, "```", "");

  out.push("## Claims", "");
  for (const c of r.claims) {
    out.push(`### ${c.id} — ${c.status}`, "");
    if (c.original_wording) out.push(`> ${c.original_wording}`, "");
    out.push(c.text, "");

    const evs = r.evidence.filter((e) => e.claim_id === c.id);
    if (evs.length > 0) {
      out.push("**Evidence**", "");
      for (const e of evs) {
        const title = e.source_title || e.source_url || "(untitled)";
        const link = e.source_url ? `[${title}](${e.source_url})` : title;
        out.push(`- ${link} — ${e.verdict} (${e.source_tier}, ${e.provider})`);
        if (e.excerpt) out.push(`  > ${e.excerpt}`);
      }
      out.push("");
    }

    const corrs = r.corrections.filter((x) => x.claim_id === c.id);
    for (const x of corrs) {
      out.push("**Correction**", "");
      out.push(`- was: ${x.original_text}`);
      out.push(`- now: ${x.corrected_text}`);
      if (x.reason) out.push(`- reason: ${x.reason}`);
      if (x.evidence_ids.length > 0) out.push(`- evidence: ${x.evidence_ids.join(", ")}`);
      out.push("");
    }
  }

  return out.join("\n");
}
