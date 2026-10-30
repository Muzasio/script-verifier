import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "./api";
import { onClaimUpdate, onProgress } from "./events";
import { normalizeStage } from "./types";
import type { Claim, FinalResult, Stage } from "./types";
import TabBar, { type TabId } from "./components/TabBar";
import InputView from "./views/InputView";
import ProgressView from "./views/ProgressView";
import ClaimsView from "./views/ClaimsView";
import ResultView from "./views/ResultView";

export default function App() {
  const [tab, setTab] = useState<TabId>("input");
  const [scriptId, setScriptId] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [stage, setStage] = useState<Stage>("idle");
  const [log, setLog] = useState<string[]>([]);
  const [claims, setClaims] = useState<Claim[]>([]);
  const [result, setResult] = useState<FinalResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const scriptIdRef = useRef<string | null>(null);
  useEffect(() => {
    scriptIdRef.current = scriptId;
  }, [scriptId]);

  const appendLog = useCallback((line: string) => {
    const ts = new Date().toLocaleTimeString(undefined, { hour12: false });
    setLog((prev) => [...prev, `${ts}  ${line}`]);
  }, []);

  const refresh = useCallback(async () => {
    const id = scriptIdRef.current;
    if (!id) return;
    try {
      const [nextClaims, nextResult] = await Promise.all([
        api.getClaims(id),
        api.getResult(id),
      ]);
      setClaims(nextClaims);
      setResult(nextResult);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    void (async () => {
      const uProgress = await onProgress((payload) => {
        if (disposed) return;
        const next = normalizeStage(payload.stage);
        setStage(next);
        if (payload.message) appendLog(payload.message);

        if (next === "done") {
          setRunning(false);
          appendLog("— verification complete —");
          void refresh().then(() => setTab("result"));
        } else if (next === "stopped" || next === "error") {
          setRunning(false);
        }
      });

      const uClaim = await onClaimUpdate((partial) => {
        if (disposed) return;
        setClaims((prev) => {
          const i = prev.findIndex((c) => c.id === partial.id);
          if (i === -1) return [...prev, partial];
          const next = prev.slice();
          next[i] = { ...next[i], ...partial };
          return next;
        });
      });

      if (disposed) {
        uProgress();
        uClaim();
        return;
      }
      unlisteners.push(uProgress, uClaim);
    })();

    return () => {
      disposed = true;
      unlisteners.forEach((u) => u());
    };
  }, [appendLog, refresh]);

  useEffect(() => {
    if ((tab === "claims" || tab === "result") && scriptId) void refresh();
  }, [tab, scriptId, refresh]);

  const handleStart = useCallback(
    async (text: string) => {
      setError(null);
      setLog([]);
      setClaims([]);
      setResult(null);
      setStage("idle");
      appendLog("starting verification…");
      try {
        const id = await api.startVerification(text);
        scriptIdRef.current = id;
        setScriptId(id);
        setRunning(true);
        setTab("progress");
        appendLog(`script_id = ${id}`);
      } catch (e) {
        setError(String(e));
        setRunning(false);
      }
    },
    [appendLog]
  );

  const handleStop = useCallback(async () => {
    const id = scriptIdRef.current;
    if (!id) return;
    try {
      await api.stopVerification(id);
    } catch (e) {
      setError(String(e));
    }
    setRunning(false);
    appendLog("stop requested");
  }, [appendLog]);

  return (
    <div className="app">
      <header className="app__header">
        <span className="app__title">Script Verifier</span>
        <TabBar active={tab} onChange={setTab} running={running} />
        <span className="app__spacer" />
        {scriptId && <span className="app__scriptid">{scriptId}</span>}
      </header>

      {error && <div className="app__error">{error}</div>}

      <main className="app__main">
        {tab === "input" && (
          <InputView running={running} onStart={handleStart} onStop={handleStop} />
        )}
        {tab === "progress" && (
          <ProgressView stage={stage} running={running} log={log} claims={claims} />
        )}
        {tab === "claims" && <ClaimsView claims={claims} result={result} />}
        {tab === "result" && <ResultView result={result} />}
      </main>
    </div>
  );
}
