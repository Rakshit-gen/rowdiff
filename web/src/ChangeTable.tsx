import { useCallback, useEffect, useRef, useState } from "react";
import { type Change, type RowFilter, type Status, getRows } from "./api";
import { RowDetail } from "./RowDetail";

const ROW = 34;
const PAGE = 200;
const OVERSCAN = 10;
const MARK: Record<Change["kind"], string> = { added: "+", removed: "−", changed: "~", duplicate: "!" };
const MARK_LABEL: Record<Change["kind"], string> = {
  added: "Added",
  removed: "Removed",
  changed: "Changed",
  duplicate: "Repeated key",
};

/**
 * Every change matching `filter`, drawn as a window over a tall scroller.
 * Only the rows on screen exist in the DOM, and rows are fetched from the
 * server a page at a time as they scroll into view.
 */
export function ChangeTable(props: { status: Status; filter: RowFilter; total: number }) {
  const { status, filter, total } = props;
  const { id, key: keyCols } = status;
  const columns = status.compared_columns ?? [];
  const [open, setOpen] = useState<Change | null>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(560);
  const [pages, setPages] = useState<Map<number, Change[]>>(new Map());
  const [error, setError] = useState<string | null>(null);
  const inFlight = useRef(new Set<number>());

  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setHeight(el.clientHeight));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const first = Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN);
  const last = Math.min(total, Math.ceil((scrollTop + height) / ROW) + OVERSCAN);

  const load = useCallback(
    (page: number) => {
      if (inFlight.current.has(page)) return;
      inFlight.current.add(page);
      getRows(id, filter, page * PAGE, PAGE)
        .then((p) => setPages((m) => new Map(m).set(page, p.rows)))
        .catch((e) => setError(e instanceof Error ? e.message : String(e)))
        .finally(() => inFlight.current.delete(page));
    },
    [id, filter],
  );

  useEffect(() => {
    for (let p = Math.floor(first / PAGE); p <= Math.floor(Math.max(first, last - 1) / PAGE); p++) {
      if (!pages.has(p)) load(p);
    }
  }, [first, last, pages, load]);

  const cols = [...keyCols, ...columns];
  const template = `2.25rem repeat(${cols.length}, minmax(9rem, 1fr))`;
  const rows = [];
  for (let i = first; i < last; i++) {
    const c = pages.get(Math.floor(i / PAGE))?.[i % PAGE];
    rows.push(
      <Row key={i} index={i} change={c} cols={cols} keyCount={keyCols.length} template={template} onOpen={setOpen} />,
    );
  }

  if (total === 0) return <p className="help empty-table">Nothing to show here.</p>;

  return (
    <div className="table" role="table" aria-rowcount={total + 1}>
      <div className="table-scroll" ref={scroller} onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}>
        <div className="tr th" role="row" style={{ gridTemplateColumns: template }}>
          <span role="columnheader" className="td mark">
            <span className="visually-hidden">Change</span>
          </span>
          {cols.map((c, n) => (
            <span role="columnheader" className={`td${n < keyCols.length ? " key" : ""}`} key={c} title={c}>
              {c}
            </span>
          ))}
        </div>
        <div style={{ height: total * ROW, position: "relative" }}>{rows}</div>
      </div>
      {error && <p className="error">{error}</p>}
      {open && <RowDetail change={open} status={status} onClose={() => setOpen(null)} />}
    </div>
  );
}

function Row(props: {
  index: number;
  change: Change | undefined;
  cols: string[];
  keyCount: number;
  template: string;
  onOpen: (c: Change) => void;
}) {
  const { change: c, cols, keyCount } = props;
  const style = { top: props.index * ROW, height: ROW, gridTemplateColumns: props.template };
  if (!c) {
    return (
      <div className="tr loading" role="row" aria-rowindex={props.index + 2} style={style}>
        <span className="td" />
      </div>
    );
  }
  const changed = c.kind === "changed" ? new Map(c.cells.map((x) => [x.column, x])) : null;
  return (
    <div
      className={`tr row-${c.kind} clickable`}
      role="row"
      tabIndex={0}
      aria-rowindex={props.index + 2}
      style={style}
      onClick={() => props.onOpen(c)}
      onKeyDown={(e) => e.key === "Enter" && props.onOpen(c)}
    >
      <span className="td mark" role="cell" title={MARK_LABEL[c.kind]}>
        <span aria-hidden>{MARK[c.kind]}</span>
        <span className="visually-hidden">{MARK_LABEL[c.kind]}</span>
      </span>
      {cols.map((col, n) => {
        const cell = changed?.get(col);
        const value = n < keyCount ? (c.key[n] ?? "") : (c.row[col] ?? "");
        if (cell) {
          return (
            <span className="td cell-changed" role="cell" key={col} title={`${cell.old} → ${cell.new}`}>
              <del>{cell.old || "(empty)"}</del> <ins>{cell.new || "(empty)"}</ins>
            </span>
          );
        }
        return (
          <span className={`td${n < keyCount ? " key" : ""}`} role="cell" key={col} title={value}>
            {value}
          </span>
        );
      })}
    </div>
  );
}
