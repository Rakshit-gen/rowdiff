// Calls to rowdiff-web. Shapes mirror src/bin/web.rs and src/output.rs.

export type Phase = "starting" | "reading" | "comparing" | "done";
export type Kind = "added" | "removed" | "changed" | "duplicate";

export interface Summary {
  rows_a: number;
  rows_b: number;
  added: number;
  removed: number;
  changed: number;
  unchanged: number;
  duplicates_a: number;
  duplicates_b: number;
  only_in_a: string[];
  only_in_b: string[];
  changed_by_column: Record<string, number>;
}

export interface Status {
  id: number;
  key: string[];
  a: { name: string; columns: string[] };
  b: { name: string; columns: string[] };
  status: "running" | "failed" | "done";
  phase: Phase;
  done: number;
  total: number;
  error?: string;
  summary?: Summary;
  compared_columns?: string[];
}

export interface Cell {
  column: string;
  old: string;
  new: string;
}

export type Change =
  | { kind: "added" | "removed"; key: string[]; row: Record<string, string> }
  | { kind: "changed"; key: string[]; row: Record<string, string>; cells: Cell[] }
  | { kind: "duplicate"; file: "a" | "b"; key: string[]; row: Record<string, string> };

export interface Page {
  total: number;
  offset: number;
  rows: Change[];
}

export interface DiffRequest {
  a: File;
  b: File;
  key: string[];
  ignore: string[];
  trim: boolean;
  ignoreCase: boolean;
  tolerance: string;
  delimiter: string;
}

async function json<T>(res: Response): Promise<T> {
  const body = await res.json().catch(() => null);
  if (!res.ok) throw new Error(body?.error ?? `The server answered ${res.status}.`);
  return body as T;
}

/** Upload both files and start the diff. `onUpload` gets a 0..1 fraction. */
export function startDiff(req: DiffRequest, onUpload: (fraction: number) => void): Promise<Status> {
  const form = new FormData();
  // Options first, files last, so the server has them before the big parts.
  for (const k of req.key) form.append("key", k);
  for (const c of req.ignore) form.append("ignore", c);
  form.append("trim", String(req.trim));
  form.append("ignore_case", String(req.ignoreCase));
  form.append("tolerance", req.tolerance.trim());
  form.append("delimiter", req.delimiter);
  form.append("a", req.a, req.a.name);
  form.append("b", req.b, req.b.name);

  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open("POST", "/api/diffs");
    xhr.responseType = "json";
    xhr.upload.onprogress = (e) => e.lengthComputable && onUpload(e.loaded / e.total);
    xhr.onload = () => {
      if (xhr.status >= 200 && xhr.status < 300) resolve(xhr.response as Status);
      else reject(new Error(xhr.response?.error ?? `The server answered ${xhr.status}.`));
    };
    xhr.onerror = () => reject(new Error("Couldn't reach rowdiff-web. Is it still running?"));
    xhr.send(form);
  });
}

export const getStatus = (id: number) => fetch(`/api/diffs/${id}`).then((r) => json<Status>(r));

export interface RowFilter {
  kind?: Kind | "all";
  column?: string;
}

export function getRows(id: number, filter: RowFilter, offset: number, limit: number): Promise<Page> {
  const q = new URLSearchParams({ offset: String(offset), limit: String(limit) });
  if (filter.column) q.set("column", filter.column);
  else if (filter.kind) q.set("kind", filter.kind);
  return fetch(`/api/diffs/${id}/rows?${q}`).then((r) => json<Page>(r));
}

export const exportUrl = (id: number) => `/api/diffs/${id}/changes.csv`;

export const deleteDiff = (id: number) => fetch(`/api/diffs/${id}`, { method: "DELETE" });
