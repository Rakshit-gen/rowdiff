import type { Status } from "./api";

const STEP: Record<string, string> = {
  starting: "Starting",
  reading_a: "Reading and sorting the older file",
  reading_b: "Reading and sorting the newer file",
  comparing: "Comparing rows",
  done: "Finishing up",
};

/** One bar for whatever is happening now: the upload, then each server phase. */
export function Running(props: { upload: number; status: Status | null }) {
  const { status } = props;
  let label: string;
  let fraction: number;
  if (!status) {
    label = "Uploading";
    fraction = props.upload;
  } else {
    label = STEP[status.phase] ?? "Working";
    fraction = status.total > 0 ? status.done / status.total : 0;
  }
  const pct = Math.min(100, Math.round(fraction * 100));
  return (
    <section className="running" aria-live="polite">
      <div className="running-label">
        {label}
        {status && status.phase !== "starting" && status.phase !== "done" && (
          <span className="help"> ({status.phase === "reading_b" ? status.b.name : status.a.name})</span>
        )}
      </div>
      <div className="bar" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100}>
        <div className="bar-fill" style={{ width: `${pct}%` }} />
      </div>
      <div className="help">{pct}%</div>
    </section>
  );
}
