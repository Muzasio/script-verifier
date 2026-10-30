import type { Claim, Correction, Evidence } from "../types";
import StatusBadge from "../components/StatusBadge";

interface Props {
  claim: Claim;
  evidence: Evidence[];
  corrections: Correction[];
}

export default function EvidencePanel({ claim, evidence, corrections }: Props) {
  return (
    <div className="panel">
      <div className="panel__head">
        <span className="mono">{claim.id}</span>
        <StatusBadge status={claim.status} />
      </div>

      <p className="panel__claim">{claim.text}</p>
      {claim.original_wording && (
        <p className="muted small">
          original: <span className="mono">{claim.original_wording}</span>
        </p>
      )}
      {claim.context && <p className="muted small">context: {claim.context}</p>}

      <h3 className="panel__h">Evidence ({evidence.length})</h3>
      {evidence.length === 0 && <div className="empty small">none</div>}
      {evidence.map((e, i) => (
        <div className="ev" key={e.id ?? `${e.claim_id}-${i}`}>
          {e.source_url ? (
            <a className="ev__link" href={e.source_url} target="_blank" rel="noreferrer">
              {e.source_title || e.source_url}
            </a>
          ) : (
            <span className="ev__link">{e.source_title || "(no source)"}</span>
          )}
          <div className="ev__meta">
            <span className={`chip chip--${e.verdict}`}>{e.verdict}</span>
            <span className="muted small">{e.source_tier}</span>
            <span className="muted small">{e.provider}</span>
          </div>
          {e.excerpt && <blockquote className="ev__excerpt">{e.excerpt}</blockquote>}
        </div>
      ))}

      <h3 className="panel__h">Correction</h3>
      {corrections.length === 0 && <div className="empty small">none</div>}
      {corrections.map((c, i) => (
        <div className="corr" key={i}>
          <div className="corr__row">
            <span className="muted small">was</span>
            <s className="mono">{c.original_text}</s>
          </div>
          <div className="corr__row">
            <span className="muted small">now</span>
            <span className="mono">{c.corrected_text}</span>
          </div>
          {c.reason && <div className="muted small corr__reason">{c.reason}</div>}
        </div>
      ))}
    </div>
  );
}
