import { useId, useState } from "react";
import type { DiffRequest } from "./api";
import { type FileHeader, guessKey, readHeader } from "./csv";
import { bytes, plural } from "./format";

interface Picked {
  file: File;
  header: FileHeader;
}

const DELIMITERS: [string, string][] = [
  [",", "Comma"],
  [";", "Semicolon"],
  ["\t", "Tab"],
  ["|", "Pipe"],
];

function FileSlot(props: {
  label: string;
  hint: string;
  picked: Picked | null;
  error: string | null;
  onFile: (f: File) => void;
}) {
  const id = useId();
  const [over, setOver] = useState(false);
  const { picked } = props;
  return (
    <div
      className={`slot${over ? " slot-over" : ""}${picked ? " slot-filled" : ""}`}
      onDragOver={(e) => {
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setOver(false);
        const f = e.dataTransfer.files[0];
        if (f) props.onFile(f);
      }}
    >
      <div className="slot-label">{props.label}</div>
      {picked ? (
        <>
          <div className="slot-name">{picked.file.name}</div>
          <div className="slot-meta">
            {bytes(picked.file.size)}, {plural(picked.header.columns.length, "column")}
          </div>
        </>
      ) : (
        <div className="slot-empty">{props.hint}</div>
      )}
      <input
        id={id}
        className="visually-hidden"
        type="file"
        accept=".csv,.tsv,.txt,text/csv"
        onChange={(e) => {
          const f = e.target.files?.[0];
          if (f) props.onFile(f);
          e.target.value = "";
        }}
      />
      <label htmlFor={id} className="button button-quiet">
        {picked ? "Choose a different file" : "Choose a file"}
      </label>
      {props.error && <p className="slot-error">{props.error}</p>}
    </div>
  );
}

export function Setup(props: { busy: boolean; onStart: (req: DiffRequest) => void }) {
  const [a, setA] = useState<Picked | null>(null);
  const [b, setB] = useState<Picked | null>(null);
  const [errors, setErrors] = useState<[string | null, string | null]>([null, null]);
  const [key, setKey] = useState<string[]>([]);
  const [ignore, setIgnore] = useState<string[]>([]);
  const [trim, setTrim] = useState(false);
  const [ignoreCase, setIgnoreCase] = useState(false);
  const [tolerance, setTolerance] = useState("");
  const [delimiter, setDelimiter] = useState<string | null>(null);

  const shared = a && b ? a.header.columns.filter((c) => b.header.columns.includes(c)) : [];
  const delim = delimiter ?? a?.header.delimiter ?? ",";
  const delimMismatch = a && b && a.header.delimiter !== b.header.delimiter;
  const toleranceBad = tolerance.trim() !== "" && !Number.isFinite(Number(tolerance));

  async function pick(side: 0 | 1, file: File) {
    try {
      const header = await readHeader(file);
      if (header.columns.length < 2 && header.columns[0] === "") throw new Error("This file looks empty.");
      const next = { file, header };
      const other = side === 0 ? b : a;
      if (side === 0) setA(next);
      else setB(next);
      setErrors((e) => (side === 0 ? [null, e[1]] : [e[0], null]));
      setDelimiter(null);
      if (other) {
        const common = next.header.columns.filter((c) => other.header.columns.includes(c));
        setKey((k) => {
          const kept = k.filter((c) => common.includes(c));
          if (kept.length) return kept;
          const g = guessKey(common);
          return g ? [g] : [];
        });
        setIgnore((ig) => ig.filter((c) => common.includes(c)));
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setErrors((err) => (side === 0 ? [msg, err[1]] : [err[0], msg]));
    }
  }

  const toggle = (list: string[], c: string) => (list.includes(c) ? list.filter((x) => x !== c) : [...list, c]);

  let blocker: string | null = null;
  if (!a || !b) blocker = "Pick both files to compare.";
  else if (shared.length === 0) blocker = "These files have no column names in common, so rows can't be matched.";
  else if (key.length === 0) blocker = "Pick the column that identifies a row.";
  else if (toleranceBad) blocker = "The number tolerance has to be a number.";

  return (
    <form
      className="setup"
      onSubmit={(e) => {
        e.preventDefault();
        if (blocker || !a || !b) return;
        props.onStart({ a: a.file, b: b.file, key, ignore, trim, ignoreCase, tolerance, delimiter: delim });
      }}
    >
      <div className="slots">
        <FileSlot
          label="Older file"
          hint="Drop the earlier export here, for example yesterday's."
          picked={a}
          error={errors[0]}
          onFile={(f) => pick(0, f)}
        />
        <FileSlot
          label="Newer file"
          hint="Drop the later export here."
          picked={b}
          error={errors[1]}
          onFile={(f) => pick(1, f)}
        />
      </div>

      {a && b && shared.length > 0 && (
        <>
          <fieldset className="field">
            <legend>Match rows by</legend>
            <p className="help">
              Pick the column that identifies a row, like an order number. Pick more than one if it takes several
              together.
            </p>
            <div className="chips">
              {shared.map((c) => (
                <button
                  type="button"
                  key={c}
                  className="chip"
                  aria-pressed={key.includes(c)}
                  onClick={() => {
                    setKey((k) => toggle(k, c));
                    setIgnore((ig) => ig.filter((x) => x !== c));
                  }}
                >
                  {c}
                </button>
              ))}
            </div>
          </fieldset>

          <details className="field options">
            <summary>Matching options</summary>
            <label className="check">
              <input type="checkbox" checked={trim} onChange={(e) => setTrim(e.target.checked)} />
              Ignore spaces at the start and end of values
            </label>
            <label className="check">
              <input type="checkbox" checked={ignoreCase} onChange={(e) => setIgnoreCase(e.target.checked)} />
              Ignore upper and lower case
            </label>
            <label className="inline">
              Treat numbers as equal when they differ by at most
              <input
                className="input input-short"
                inputMode="decimal"
                placeholder="0.01"
                value={tolerance}
                aria-invalid={toleranceBad}
                onChange={(e) => setTolerance(e.target.value)}
              />
            </label>
            <p className="help">Leave it empty to compare numbers as text. 0 still makes 1.0 and 1 equal.</p>
            <label className="inline">
              Delimiter
              <select className="input" value={delim} onChange={(e) => setDelimiter(e.target.value)}>
                {DELIMITERS.map(([v, name]) => (
                  <option key={name} value={v}>
                    {name}
                  </option>
                ))}
              </select>
            </label>
            {delimMismatch && (
              <p className="warn">
                The two files seem to use different delimiters. Both are read with the one picked here.
              </p>
            )}
            <div className="field-sub">
              <div className="label">Leave these columns out</div>
              <div className="chips">
                {shared
                  .filter((c) => !key.includes(c))
                  .map((c) => (
                    <button
                      type="button"
                      key={c}
                      className="chip"
                      aria-pressed={ignore.includes(c)}
                      onClick={() => setIgnore((ig) => toggle(ig, c))}
                    >
                      {c}
                    </button>
                  ))}
              </div>
            </div>
          </details>
        </>
      )}

      <div className="actions">
        <button type="submit" className="button" disabled={!!blocker || props.busy}>
          Compare
        </button>
        {blocker && a && b && <span className="help">{blocker}</span>}
      </div>
    </form>
  );
}
