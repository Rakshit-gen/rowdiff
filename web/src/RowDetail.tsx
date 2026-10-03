import { useEffect, useRef } from "react";
import type { Change, Status } from "./api";

const TITLE: Record<Change["kind"], string> = {
  added: "Only in the newer file",
  removed: "Only in the older file",
  changed: "Changed",
  duplicate: "Repeated key",
};

/** Every column of one row, older value beside newer, in a native dialog. */
export function RowDetail(props: { change: Change; status: Status; onClose: () => void }) {
  const ref = useRef<HTMLDialogElement>(null);
  const { change: c, status } = props;

  useEffect(() => {
    ref.current?.showModal();
  }, []);

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

  return (
    <dialog ref={ref} className="detail" onClose={props.onClose} onClick={(e) => e.target === ref.current && ref.current?.close()}>
      <div className="detail-head">
        <h2>
          {TITLE[c.kind]}: <code>{c.key.join(", ")}</code>
        </h2>
        <button type="button" className="button button-quiet" onClick={() => ref.current?.close()}>
          Close
        </button>
      </div>
      <table className="detail-table">
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
    </dialog>
  );
}
