import { useState } from "react";
import * as api from "../api";

interface Props {
  running: boolean;
  onStart: (scriptText: string) => void;
  onStop: () => void;
}

export default function InputView({ running, onStart, onStop }: Props) {
  const [text, setText] = useState("");

  




  async function handleDiag(provider: "claude" | "gemini" | "deepseek") {
    try {
      const dump = await api.diagnosePage(provider);
      const path = await api.writeDiagFile(provider, dump);
      alert(`${provider} diagnosis saved to:\n${path}`);
    } catch (e) {
      alert(`diagnose ${provider} failed: ${String(e)}`);
    }
  }

  async function handleClear(provider: "claude" | "gemini" | "deepseek") {
    if (!confirm(`Delete all ${provider} chats? This cannot be undone.`)) return;
    try {
      const n = await api.clearChats(provider);
      alert(`${provider}: deleted ${n} chats`);
    } catch (e) {
      alert(`${provider}: FAILED — ${String(e)}`);
    }
  }

  async function handleClearAll() {
    if (!confirm("Delete ALL chats on Claude, Gemini, and DeepSeek? This cannot be undone.")) return;
    const results: string[] = [];
    for (const p of ["claude", "gemini", "deepseek"] as const) {
      try {
        const n = await api.clearChats(p);
        results.push(`${p}: ${n} deleted`);
      } catch (e) {
        results.push(`${p}: FAILED — ${String(e)}`);
      }
    }
    alert("Cleanup complete:\n\n" + results.join("\n"));
  }

return (
    <div className="view">
      <label className="label" htmlFor="script-input">
        Script
      </label>
      <textarea
        id="script-input"
        className="textarea textarea--input mono"
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder="Paste the script to verify…"
        spellCheck={false}
        disabled={running}
      />
      <div className="row">
        <button
          type="button"
          className="btn btn--primary"
          disabled={running || text.trim().length === 0}
          onClick={() => onStart(text)}
        >
          Start Verification
        </button>
        <button type="button" className="btn" disabled={!running} onClick={onStop}>
          Stop
        </button>
        <span className="muted small">{text.length} chars</span>
      </div>
      <div className="row cleanup-row">
        <span className="muted small">Maintenance:</span>
        <button type="button" className="btn btn--danger" onClick={handleClearAll}>
          Clear all chats
        </button>
        <button type="button" className="btn" onClick={() => handleClear("claude")}>Clear Claude</button>
        <button type="button" className="btn" onClick={() => handleClear("gemini")}>Clear Gemini</button>
        <button type="button" className="btn" onClick={() => handleClear("deepseek")}>Clear DeepSeek</button>
        <span className="muted small">·</span>
        <button type="button" className="btn btn--ghost btn--small" onClick={() => handleDiag("claude")}>Diag</button>
        <button type="button" className="btn btn--ghost btn--small" onClick={() => handleDiag("gemini")}>Diag</button>
        <button type="button" className="btn btn--ghost btn--small" onClick={() => handleDiag("deepseek")}>Diag</button>
      </div>

    </div>
  );
}
