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
  // Roving focus: one row is in the tab order, the arrow keys move it.
  const [active, setActive] = useState(0);
  const [open, setOpen] = useState<number | null>(null);
  const focusAfterMove = useRef(false);
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

  const at = (i: number) => pages.get(Math.floor(i / PAGE))?.[i % PAGE];

  /** Make row `i` the active one and scroll it into view below the sticky header. */
  const moveTo = (i: number, focus: boolean) => {
    const next = Math.max(0, Math.min(total - 1, i));
    const el = scroller.current;
    if (el) {
      const top = next * ROW;
      if (top < el.scrollTop) el.scrollTop = top;
      else if (top + ROW > el.scrollTop + el.clientHeight - ROW) el.scrollTop = top + 2 * ROW - el.clientHeight;
    }
    focusAfterMove.current = focus;
    setActive(next);
    return next;
  };

  useEffect(() => {
    if (!focusAfterMove.current) return;
    focusAfterMove.current = false;
    scroller.current?.querySelector<HTMLElement>(`[data-i="${active}"]`)?.focus({ preventScroll: true });
  }, [active, pages]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    const step: Record<string, number> = { ArrowDown: 1, j: 1, ArrowUp: -1, k: -1, PageDown: 10, PageUp: -10 };
    if (e.key in step) moveTo(active + step[e.key]!, true);
    else if (e.key === "Home") moveTo(0, true);
    else if (e.key === "End") moveTo(total - 1, true);
    else if (e.key === "Enter" && at(active)) setOpen(active);
    else return;
    e.preventDefault();
  };

  const cols = [...keyCols, ...columns];
  const template = `2.25rem repeat(${cols.length}, minmax(9rem, 1fr))`;
  const rows = [];
  for (let i = first; i < last; i++) {
    rows.push(
      <Row
        key={i}
        index={i}
        active={i === active}
        change={at(i)}
        cols={cols}
        keyCount={keyCols.length}
        template={template}
        onOpen={() => {
          setActive(i);
          setOpen(i);
        }}
        onFocus={() => setActive(i)}
      />,
    );
  }

  if (total === 0) return <p className="help empty-table">Nothing to show here.</p>;

  return (
    <div className="table" role="table" aria-rowcount={total + 1}>
      <div
        className="table-scroll"
        ref={scroller}
        onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
        onKeyDown={onKeyDown}
        // When the active row has scrolled out of the DOM, the scroller itself
        // takes the tab stop so the keys still work.
        tabIndex={active < first || active >= last ? 0 : -1}
        aria-label="Changes"
      >
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
      {open !== null && (
        <RowDetail
          change={at(open)}
          index={open}
          total={total}
          status={status}
          onStep={(d) => setOpen(moveTo(open + d, false))}
          onClose={() => {
            setOpen(null);
            moveTo(active, true);
          }}
        />
      )}
    </div>
  );
}

function Row(props: {
  index: number;
  active: boolean;
  change: Change | undefined;
  cols: string[];
  keyCount: number;
  template: string;
  onOpen: () => void;
  onFocus: () => void;
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
      data-i={props.index}
      tabIndex={props.active ? 0 : -1}
      aria-rowindex={props.index + 2}
      style={style}
      onClick={props.onOpen}
      onFocus={props.onFocus}
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
