import { useState } from "react";
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
          <div className="tabs" role="tablist">
            {TABS.filter((t) => t.kind !== "duplicate" || counts.duplicate > 0).map((t) => {
              const on = !filter.column && filter.kind === t.kind;
              return (
                <button
                  key={t.kind}
                  type="button"
                  role="tab"
                  aria-selected={on}
                  className={`tab tab-${t.kind}`}
                  onClick={() => setFilter({ kind: t.kind })}
                >
                  {t.label} <span className="tab-count">{num(counts[t.kind])}</span>
                </button>
              );
            })}
          </div>
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
            id={status.id}
            filter={filter}
            total={filter.column ? (s.changed_by_column[filter.column] ?? 0) : counts[filter.kind ?? "all"]}
            keyCols={status.key}
            columns={status.compared_columns ?? []}
          />
        </div>
      </div>
    </section>
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
