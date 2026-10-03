import type { Status } from "./api";
import { bytes, num } from "./format";

const STEPS = [
  { id: "upload", label: "Uploading" },
  { id: "reading", label: "Reading and sorting both files" },
  { id: "comparing", label: "Comparing rows" },
] as const;

type StepId = (typeof STEPS)[number]["id"];

/** The three steps of a diff, the current one with its own progress bar. */
export function Running(props: { upload: number; status: Status | null }) {
  const { status } = props;
  const current: StepId = !status ? "upload" : status.phase === "comparing" || status.phase === "done" ? "comparing" : "reading";
  const at = STEPS.findIndex((s) => s.id === current);
  const fraction = !status ? props.upload : status.total > 0 ? status.done / status.total : 0;
  const pct = Math.min(100, Math.round(fraction * 100));
  // The server is in "starting" before it knows how much work there is.
  const unknown = status?.phase === "starting";

  let detail = `${pct}%`;
  if (status?.phase === "reading" && status.total > 0) detail = `${bytes(status.done)} of ${bytes(status.total)}`;
  if (status?.phase === "comparing") detail = `${num(status.done)} of ${num(status.total)} rows`;

  return (
    <section className="running" aria-live="polite">
      <ol className="steps">
        {STEPS.map((s, i) => {
          const state = i < at ? "done" : i === at ? "now" : "later";
          return (
            <li key={s.id} className={`step step-${state}`} aria-current={state === "now" ? "step" : undefined}>
              <span className="step-dot" aria-hidden />
              <span className="step-label">
                {s.label}
                {state === "done" && <span className="visually-hidden"> (done)</span>}
              </span>
              {state === "now" && (
                <div className="step-progress">
                  <div
                    className={`bar${unknown ? " bar-unknown" : ""}`}
                    role="progressbar"
                    aria-label={s.label}
                    aria-valuenow={unknown ? undefined : pct}
                    aria-valuemin={0}
                    aria-valuemax={100}
                  >
                    <div className="bar-fill" style={{ width: unknown ? undefined : `${pct}%` }} />
                  </div>
                  <div className="help">{unknown ? "Starting" : detail}</div>
                </div>
              )}
            </li>
          );
        })}
      </ol>
      {status && (
        <p className="help">
          {status.a.name} and {status.b.name}
        </p>
      )}
    </section>
  );
}
