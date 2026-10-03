import { useEffect, useRef, useState } from "react";
import { type DiffRequest, type Status, deleteDiff, getStatus, startDiff } from "./api";
import { Results } from "./Results";
import { Running } from "./Running";
import { Setup } from "./Setup";

type View =
  | { name: "setup"; error?: string }
  | { name: "running"; upload: number; status: Status | null }
  | { name: "done"; status: Status };

export function App() {
  const [view, setView] = useState<View>({ name: "setup" });
  const polling = useRef<number | null>(null);

  useEffect(() => () => {
    if (polling.current) window.clearTimeout(polling.current);
  }, []);

  // A finished diff stays open across reloads through ?diff=<id>, for as long
  // as the server still has it.
  useEffect(() => {
    const id = Number(new URLSearchParams(window.location.search).get("diff"));
    if (!id) return;
    getStatus(id)
      .then((s) => s.status === "done" && setView({ name: "done", status: s }))
      .catch(() => window.history.replaceState(null, "", window.location.pathname));
  }, []);

  useEffect(() => {
    const url = view.name === "done" ? `?diff=${view.status.id}` : window.location.pathname;
    window.history.replaceState(null, "", url);
  }, [view]);

  async function start(req: DiffRequest) {
    setView({ name: "running", upload: 0, status: null });
    let status: Status;
    try {
      status = await startDiff(req, (upload) => setView({ name: "running", upload, status: null }));
    } catch (e) {
      setView({ name: "setup", error: e instanceof Error ? e.message : String(e) });
      return;
    }
    const poll = async (id: number) => {
      try {
        const s = await getStatus(id);
        if (s.status === "done") setView({ name: "done", status: s });
        else if (s.status === "failed") setView({ name: "setup", error: s.error ?? "The diff failed." });
        else {
          setView({ name: "running", upload: 1, status: s });
          polling.current = window.setTimeout(() => poll(id), 250);
        }
      } catch (e) {
        setView({ name: "setup", error: e instanceof Error ? e.message : String(e) });
      }
    };
    poll(status.id);
  }

  return (
    <main className="page">
      <header className="top">
        <h1>rowdiff</h1>
        <p className="lede">Compare two CSV exports by key and see which rows were added, removed or changed.</p>
      </header>
      <div hidden={view.name !== "setup"}>
        <Setup busy={view.name === "running"} onStart={start} />
        {view.name === "setup" && view.error && <p className="error">{view.error}</p>}
      </div>
      {view.name === "running" && <Running upload={view.upload} status={view.status} />}
      {view.name === "done" && (
        <Results
          status={view.status}
          onReset={() => {
            void deleteDiff(view.status.id);
            setView({ name: "setup" });
          }}
        />
      )}
    </main>
  );
}
