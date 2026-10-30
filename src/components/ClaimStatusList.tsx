import type { Claim } from "../types";
import StatusBadge from "./StatusBadge";

function truncate(s: string, n: number) {
  return s.length > n ? `${s.slice(0, n - 1)}…` : s;
}

export default function ClaimStatusList({ claims }: { claims: Claim[] }) {
  if (claims.length === 0) return <div className="empty small">no claims yet</div>;

  return (
    <ul className="claimlist">
      {claims.map((c) => (
        <li className="claimlist__row" key={c.id}>
          <span className="mono claimlist__id">{c.id}</span>
          <span className="claimlist__text">
            {truncate(c.text || c.original_wording || "", 120)}
          </span>
          <StatusBadge status={c.status} />
        </li>
      ))}
    </ul>
  );
}
