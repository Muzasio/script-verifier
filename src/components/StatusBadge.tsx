import type { ClaimStatus } from "../types";

const KNOWN: ReadonlySet<string> = new Set<ClaimStatus>([
  "pending",
  "researching",
  "verified",
  "contradicted",
  "corrected",
  "unverified",
]);

export default function StatusBadge({ status }: { status: ClaimStatus | string }) {
  const key = KNOWN.has(status) ? status : "pending";
  return <span className={`badge badge--${key}`}>{status}</span>;
}
