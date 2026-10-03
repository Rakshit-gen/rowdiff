import { Setup } from "./Setup";

export function App() {
  return (
    <main className="page">
      <header className="top">
        <h1>rowdiff</h1>
        <p className="lede">Compare two CSV exports by key and see which rows were added, removed or changed.</p>
      </header>
      <Setup busy={false} onStart={(req) => console.log(req)} />
    </main>
  );
}
