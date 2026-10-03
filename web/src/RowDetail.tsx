import { useEffect, useRef, useState } from "react";
import type { Change, Status } from "./api";

const TITLE: Record<Change["kind"], string> = {
  added: "Only in the newer file",
  removed: "Only in the older file",
  changed: "Changed",
  duplicate: "Repeated key",
};

/**
 * Every column of one row, older value beside newer, in a native dialog.
 * The arrow keys (or j and k) step through the rows of the current view.
 */
export function RowDetail(props: {
  change: Change | undefined;
  index: number;
  total: number;
  status: Status;
  onStep: (delta: number) => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const [copied, setCopied] = useState(false);
  const { change: c, status, index, total } = props;

  useEffect(() => {
    ref.current?.showModal();
  }, []);

  useEffect(() => setCopied(false), [index]);

  const step = (d: number) => {
    if (index + d >= 0 && index + d < total) props.onStep(d);
  };

  const nav = (
    <div className="detail-nav">
      <button type="button" className="button button-quiet" disabled={index === 0} onClick={() => step(-1)}>
        Previous
      </button>
      <span className="help">
        {(index + 1).toLocaleString("en-US")} of {total.toLocaleString("en-US")}
      </span>
      <button type="button" className="button button-quiet" disabled={index === total - 1} onClick={() => step(1)}>
        Next
      </button>
    </div>
  );

  const dialog = (body: React.ReactNode) => (
    <dialog
      ref={ref}
      className="detail"
      onClose={props.onClose}
      onClick={(e) => e.target === ref.current && ref.current?.close()}
      onKeyDown={(e) => {
        const d = { ArrowRight: 1, j: 1, ArrowLeft: -1, k: -1 }[e.key];
        if (d === undefined) return;
        e.preventDefault();
        step(d);
      }}
    >
      {body}
    </dialog>
  );

  if (!c) return dialog(<p className="help">Loading row {index + 1}.</p>);

  const columns = c.kind === "removed" || (c.kind === "duplicate" && c.file === "a") ? status.a.columns : status.b.columns;
  const cells = c.kind === "changed" ? new Map(c.cells.map((x) => [x.column, x])) : new Map();
  const older = (col: string) => {
    if (c.kind === "added" || (c.kind === "duplicate" && c.file === "b")) return null;
    if (c.kind === "changed" && !status.a.columns.includes(col)) return null;
    return cells.get(col)?.old ?? c.row[col] ?? "";
  };
  const newer = (col: string) => {
    if (c.kind === "removed" || (c.kind === "duplicate" && c.file === "a")) return null;
    return c.row[col] ?? "";
  };

  return dialog(
    <>
      <div className="detail-head">
        <h2>
          {TITLE[c.kind]}: <code>{c.key.join(", ")}</code>
        </h2>
        <div className="detail-actions">
          <button
            type="button"
            className="button button-quiet"
            onClick={() =>
              navigator.clipboard.writeText(c.key.join(",")).then(
                () => setCopied(true),
                () => {},
              )
            }
          >
            {copied ? "Copied" : "Copy key"}
          </button>
          <button type="button" className="button button-quiet" onClick={() => ref.current?.close()}>
            Close
          </button>
        </div>
      </div>
      <table className="detail-table" key={index}>
        <thead>
          <tr>
            <th scope="col">Column</th>
            <th scope="col">{status.a.name}</th>
            <th scope="col">{status.b.name}</th>
          </tr>
        </thead>
        <tbody>
          {columns.map((col) => {
            const [o, n] = [older(col), newer(col)];
            const diff = cells.has(col);
            return (
              <tr key={col} className={diff ? "detail-changed" : undefined}>
                <th scope="row">{col}</th>
                <td>{o === null ? <span className="help">not in this file</span> : diff ? <del>{o}</del> : o}</td>
                <td>{n === null ? <span className="help">not in this file</span> : diff ? <ins>{n}</ins> : n}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
      {nav}
    </>,
  );
}
