import { useLayoutEffect, useRef, useState } from "react";
import { type Kind, type RowFilter, type Status, exportUrl } from "./api";
import { ChangeTable } from "./ChangeTable";
import { num, plural } from "./format";

const TABS: { kind: Kind | "all"; label: string }[] = [
  { kind: "all", label: "All" },
  { kind: "changed", label: "Changed" },
  { kind: "added", label: "Added" },
  { kind: "removed", label: "Removed" },
  { kind: "duplicate", label: "Repeated keys" },
];

export function Results(props: { status: Status; onReset: () => void }) {
  const { status } = props;
  const s = status.summary!;
  const [filter, setFilter] = useState<RowFilter>({ kind: "all" });
  const counts: Record<Kind | "all", number> = {
    added: s.added,
    removed: s.removed,
    changed: s.changed,
    duplicate: s.duplicates_a + s.duplicates_b,
    all: s.added + s.removed + s.changed + s.duplicates_a + s.duplicates_b,
  };
  const byColumn = Object.entries(s.changed_by_column)
    .filter(([, n]) => n > 0)
    .sort((x, y) => y[1] - x[1]);
  const top = byColumn[0]?.[1] ?? 0;

  const header = (
    <div className="results-head">
      <p className="summary-line">
        <span className="file">{status.a.name}</span> has {plural(s.rows_a, "row")},{" "}
        <span className="file">{status.b.name}</span> has {plural(s.rows_b, "row")}.{" "}
        {counts.all === 0
          ? "No differences."
          : `${num(s.unchanged)} ${s.unchanged === 1 ? "row is" : "rows are"} the same in both.`}
      </p>
      <div className="results-actions">
        {counts.all > 0 && (
          <a className="button button-quiet" href={exportUrl(status.id)} download>
            Download changes as CSV
          </a>
        )}
        <button type="button" className="button button-quiet" onClick={props.onReset}>
          Compare other files
        </button>
      </div>
    </div>
  );

  if (counts.all === 0) {
    return (
      <section className="results">
        {header}
        <p className="nothing">
          Every row in one file has a row with the same key in the other, and the values match
          {s.only_in_a.length + s.only_in_b.length > 0 ? " in the columns both files have." : "."}
        </p>
        <SchemaNote status={status} />
      </section>
    );
  }

  return (
    <section className="results">
      {header}
      <Split counts={counts} unchanged={s.unchanged} filter={filter} onPick={(kind) => setFilter({ kind })} />
      <SchemaNote status={status} />
      <div className="results-body">
        <aside className="columns-panel">
          <h2>Changed rows by column</h2>
          {byColumn.length === 0 ? (
            <p className="help">No values changed in rows that are in both files.</p>
          ) : (
            <ul>
              {byColumn.map(([name, n]) => {
                const on = filter.column === name;
                return (
                  <li key={name}>
                    <button
                      type="button"
                      className="column-row"
                      aria-pressed={on}
                      onClick={() => setFilter(on ? { kind: "changed" } : { column: name })}
                    >
                      <span className="column-name">{name}</span>
                      <span className="column-count">{num(n)}</span>
                      <span className="column-bar" style={{ width: `${(n / top) * 100}%` }} />
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </aside>
        <div className="table-panel">
          <Tabs filter={filter} counts={counts} onPick={(kind) => setFilter({ kind })} />
          {filter.column && (
            <p className="filter-note">
              Showing rows where <code>{filter.column}</code> changed.{" "}
              <button type="button" className="link" onClick={() => setFilter({ kind: "changed" })}>
                Show all changed rows
              </button>
            </p>
          )}
          <ChangeTable
            key={JSON.stringify(filter)}
            status={status}
            filter={filter}
            total={filter.column ? (s.changed_by_column[filter.column] ?? 0) : counts[filter.kind ?? "all"]}
          />
          <p className="help">
            Click a row, or move with the arrow keys or j and k and press Enter, to see every column side by side.
            The same keys step through rows there.
          </p>
        </div>
      </div>
    </section>
  );
}

const SEGMENTS: { kind: Kind; label: string }[] = [
  { kind: "changed", label: "changed" },
  { kind: "added", label: "added" },
  { kind: "removed", label: "removed" },
];

/**
 * The rows that differ as one bar, split by kind. Matching rows are left out
 * of the bar so a small diff in a big file is still readable; the caption
 * says how small it is. Each part filters the table.
 */
function Split(props: {
  counts: Record<Kind | "all", number>;
  unchanged: number;
  filter: RowFilter;
  onPick: (k: Kind) => void;
}) {
  const { counts, unchanged } = props;
  const differ = counts.changed + counts.added + counts.removed;
  if (differ === 0) return null;
  const share = (differ / (differ + unchanged)) * 100;
  return (
    <div className="split-wrap">
      <div className="split">
        {SEGMENTS.filter((g) => counts[g.kind] > 0).map((g) => (
          <button
            key={g.kind}
            type="button"
            className={`split-part split-${g.kind}`}
            style={{ flexGrow: counts[g.kind] }}
            aria-pressed={!props.filter.column && props.filter.kind === g.kind}
            onClick={() => props.onPick(g.kind)}
          >
            <span className="split-text">
              {num(counts[g.kind])} {g.label}
            </span>
          </button>
        ))}
      </div>
      <p className="help">
        {plural(differ, "row")} differ, {share < 0.1 ? "under 0.1" : share.toFixed(1)}% of the{" "}
        {num(differ + unchanged)} keys in either file.
      </p>
    </div>
  );
}

/** The kind tabs, with an underline that slides to the selected one. */
function Tabs(props: { filter: RowFilter; counts: Record<Kind | "all", number>; onPick: (k: Kind | "all") => void }) {
  const { filter, counts } = props;
  const list = useRef<HTMLDivElement>(null);
  const [mark, setMark] = useState<{ left: number; width: number } | null>(null);
  const selected = filter.column ? null : filter.kind;

  useLayoutEffect(() => {
    const el = list.current?.querySelector<HTMLElement>('[aria-selected="true"]');
    setMark(el ? { left: el.offsetLeft, width: el.offsetWidth } : null);
  }, [selected]);

  return (
    <div className="tabs" role="tablist" ref={list}>
      {TABS.filter((t) => t.kind !== "duplicate" || counts.duplicate > 0).map((t) => (
        <button
          key={t.kind}
          type="button"
          role="tab"
          aria-selected={selected === t.kind}
          className={`tab tab-${t.kind}`}
          onClick={() => props.onPick(t.kind)}
        >
          {t.label} <span className="tab-count">{num(counts[t.kind])}</span>
        </button>
      ))}
      {mark && <span className="tab-mark" style={{ transform: `translateX(${mark.left}px)`, width: mark.width }} />}
    </div>
  );
}

function SchemaNote({ status }: { status: Status }) {
  const s = status.summary!;
  if (s.only_in_a.length + s.only_in_b.length === 0) return null;
  return (
    <div className="schema-note">
      {s.only_in_a.length > 0 && (
        <p>
          Only in {status.a.name}: <Cols names={s.only_in_a} />.
        </p>
      )}
      {s.only_in_b.length > 0 && (
        <p>
          Only in {status.b.name}: <Cols names={s.only_in_b} />.
        </p>
      )}
      <p className="help">Columns that only one file has are listed here and left out of the comparison.</p>
    </div>
  );
}

function Cols({ names }: { names: string[] }) {
  return (
    <>
      {names.map((n, i) => (
        <span key={n}>
          {i > 0 && ", "}
          <code>{n}</code>
        </span>
      ))}
    </>
  );
}
