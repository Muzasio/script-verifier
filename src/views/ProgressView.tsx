import type { Claim, Stage } from "../types";
import { PIPELINE_STAGES } from "../types";
import LogPanel from "../components/LogPanel";
import ClaimStatusList from "../components/ClaimStatusList";

interface Props {
  stage: Stage;
  running: boolean;
  log: string[];
  claims: Claim[];
}

export default function ProgressView({ stage, running, log, claims }: Props) {
  const currentIndex = PIPELINE_STAGES.findIndex((s) => s.id === stage);

  return (
    <div className="view">
      <section className="block">
        <div className="block__head">
          <h2 className="block__title">Stage</h2>
          <span className={`runflag ${running ? "runflag--on" : ""}`}>
            {running ? "running" : "idle"}
          </span>
        </div>
        <ol className="stages">
          {PIPELINE_STAGES.map((s, i) => {
            const state =
              currentIndex === -1
                ? "todo"
                : i < currentIndex
                ? "done"
                : i === currentIndex
                ? "active"
                : "todo";
            return (
              <li key={s.id} className={`stage stage--${state}`}>
                <span className="stage__marker" aria-hidden />
                <span className="stage__label">{s.label}</span>
              </li>
            );
          })}
        </ol>
      </section>

      <section className="block block--grow">
        <h2 className="block__title">Log</h2>
        <LogPanel lines={log} />
      </section>

      <section className="block">
        <h2 className="block__title">Claims ({claims.length})</h2>
        <ClaimStatusList claims={claims} />
      </section>
    </div>
  );
}
