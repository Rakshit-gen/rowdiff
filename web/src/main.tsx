import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/source-sans-3";
import "@fontsource/source-code-pro/400.css";
import "@fontsource/source-code-pro/600.css";
import "./app.css";
import { App } from "./App";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
