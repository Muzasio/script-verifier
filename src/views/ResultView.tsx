import { useState } from "react";
import type { Claim, Correction, Evidence, FinalResult } from "../types";
import { resultToMarkdown } from "../markdown";

interface Props {
  result: FinalResult | null;
}

export default function ResultView({ result }: Props) {
  const [copied, setCopied] = useState(false);

  if (!result) {
    return (
      <div className="view">
        <div className="empty">no result yet</div>
      </div>
    );
  }

  const r: FinalResult = result;
  const isPass = r.verdict === "PASS";

  const correctionsByClaim = new Map<string, Correction>();
  for (const c of r.corrections) correctionsByClaim.set(c.claim_id, c);

  const evidenceByClaim = new Map<string, Evidence[]>();
  for (const e of r.evidence) {
    const arr = evidenceByClaim.get(e.claim_id) ?? [];
    arr.push(e);
    evidenceByClaim.set(e.claim_id, arr);
  }

  const handleCopyScript = async () => {
    try {
      await navigator.clipboard.writeText(r.final_script);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {}
  };

  const handleCopyMd = async () => {
    try {
      await navigator.clipboard.writeText(resultToMarkdown(r));
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {}
  };

  const handleExport = () => {
    const blob = new Blob([resultToMarkdown(r)], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${r.script_id || "result"}.md`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  return (
    <div className="view result">
      <div className={`verdict ${isPass ? "verdict--pass" : "verdict--review"}`}>
        <span className="verdict__label">{isPass ? "PASS" : "NEEDS REVIEW"}</span>
        <span className="verdict__meta">
          {r.claims.length} claims · {r.evidence.length} evidence ·{" "}
          {r.corrections.length} corrections
        </span>
        {r.unverified_claim_ids.length > 0 && (
          <span className="verdict__sub">
            {r.unverified_claim_ids.length} unverified
          </span>
        )}
      </div>

      <div className="row">
        <button type="button" className="btn" onClick={handleCopyScript}>
          {copied ? "Copied" : "Copy script"}
        </button>
        <button type="button" className="btn" onClick={handleCopyMd}>
          Copy as .md
        </button>
        <button type="button" className="btn" onClick={handleExport}>
          Export .md
        </button>
      </div>

      <section className="block">
        <h2 className="block__title">Final script</h2>
        <textarea
          className="textarea textarea--result mono"
          value={r.final_script}
          readOnly
          spellCheck={false}
        />
      </section>

      <section className="block">
        <h2 className="block__title">Claim-by-claim</h2>
        <div className="claim-diffs">
          {r.claims.map((c) => (
            <ClaimDiff
              key={c.id}
              claim={c}
              evidence={evidenceByClaim.get(c.id) ?? []}
              correction={correctionsByClaim.get(c.id)}
            />
          ))}
        </div>
      </section>
    </div>
  );
}

function ClaimDiff({
  claim,
  evidence,
  correction,
}: {
  claim: Claim;
  evidence: Evidence[];
  correction?: Correction;
}) {
  const [open, setOpen] = useState(false);
  const supports = evidence.filter((e) => e.verdict === "supports").length;
  const contradicts = evidence.filter((e) => e.verdict === "contradicts").length;
  const insufficient = evidence.filter((e) => e.verdict === "insufficient").length;

  return (
    <div className={`diff diff--${claim.status}`}>
      <div className="diff__head">
        <span className="mono diff__id">{claim.id}</span>
        <span className={`badge badge--${claim.status}`}>{claim.status}</span>
        <span className="diff__counts">
          <span className="chip chip--supports">{supports}</span>
          <span className="chip chip--contradicts">{contradicts}</span>
          <span className="chip chip--insufficient">{insufficient}</span>
        </span>
        <button
          type="button"
          className="btn btn--ghost btn--small"
          onClick={() => setOpen((v) => !v)}
        >
          {open ? "Hide evidence" : `Show ${evidence.length} evidence`}
        </button>
      </div>

      <div className="diff__body">
        {correction ? (
          <>
            <div className="diff__row diff__row--before">
              <span className="diff__label">before</span>
              <span className="diff__text diff__text--before">
                {correction.original_text}
              </span>
            </div>
            <div className="diff__row diff__row--after">
              <span className="diff__label">after</span>
              <span className="diff__text diff__text--after">
                {correction.corrected_text}
              </span>
            </div>
            {correction.reason && (
              <div className="diff__reason">
                <span className="muted small">reason:</span> {correction.reason}
              </div>
            )}
          </>
        ) : (
          <div className="diff__row">
            <span className="diff__label">claim</span>
            <span className="diff__text">{claim.original_wording}</span>
          </div>
        )}
      </div>

      {open && (
        <ul className="diff__evidence">
          {evidence.map((e, i) => (
            <li className="diff__ev" key={e.id ?? `${claim.id}-${i}`}>
              <span className={`chip chip--${e.verdict}`}>{e.verdict}</span>
              <span className="muted small">{e.source_tier}</span>
              {e.source_url ? (
                <a
                  className="diff__ev-link"
                  href={e.source_url}
                  target="_blank"
                  rel="noreferrer"
                >
                  {e.source_title || e.source_url}
                </a>
              ) : (
                <span>{e.source_title || "(no source)"}</span>
              )}
              {e.excerpt && <div className="diff__ev-excerpt">{e.excerpt}</div>}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
