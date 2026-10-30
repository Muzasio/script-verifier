export type TabId = "input" | "progress" | "claims" | "result";

const TABS: ReadonlyArray<{ id: TabId; label: string }> = [
  { id: "input", label: "Input" },
  { id: "progress", label: "Progress" },
  { id: "claims", label: "Claims" },
  { id: "result", label: "Result" },
];

interface Props {
  active: TabId;
  onChange: (tab: TabId) => void;
  running: boolean;
}

export default function TabBar({ active, onChange, running }: Props) {
  return (
    <nav className="tabs">
      {TABS.map((t) => (
        <button
          key={t.id}
          type="button"
          className={`tab${active === t.id ? " tab--active" : ""}`}
          onClick={() => onChange(t.id)}
        >
          {t.label}
          {t.id === "progress" && running && <span className="tab__dot" aria-hidden />}
        </button>
      ))}
    </nav>
  );
}
