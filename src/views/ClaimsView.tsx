import { useState } from "react";
import type { Claim, FinalResult } from "../types";
import StatusBadge from "../components/StatusBadge";
import EvidencePanel from "./EvidencePanel";

interface Props {
  claims: Claim[];
  result: FinalResult | null;
}

export default function ClaimsView({ claims, result }: Props) {
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const evidence = result?.evidence ?? [];
  const corrections = result?.corrections ?? [];
  const selected = claims.find((c) => c.id === selectedId) ?? null;

  const countFor = (claimId: string) =>
    evidence.reduce((n, e) => (e.claim_id === claimId ? n + 1 : n), 0);

  return (
    <div className="view view--split">
      <div className="split__main">
        {claims.length === 0 ? (
          <div className="empty">no claims</div>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th style={{ width: 90 }}>id</th>
                <th>original_wording</th>
                <th style={{ width: 130 }}>status</th>
                <th style={{ width: 80 }} className="num">
                  evidence
                </th>
              </tr>
            </thead>
            <tbody>
              {claims.map((c) => (
                <tr
                  key={c.id}
                  className={selectedId === c.id ? "row--selected" : undefined}
                  onClick={() => setSelectedId(c.id)}
                >
                  <td className="mono">{c.id}</td>
                  <td>{c.original_wording}</td>
                  <td>
                    <StatusBadge status={c.status} />
                  </td>
                  <td className="num">{countFor(c.id)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      <aside className="split__side">
        {selected ? (
          <EvidencePanel
            claim={selected}
            evidence={evidence.filter((e) => e.claim_id === selected.id)}
            corrections={corrections.filter((x) => x.claim_id === selected.id)}
          />
        ) : (
          <div className="empty">select a claim</div>
        )}
      </aside>
    </div>
  );
}
