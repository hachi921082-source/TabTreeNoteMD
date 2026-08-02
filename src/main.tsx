import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./App.css";

// App-wide native-context-menu suppression (`docs/plans/window-operation/PLAN.md` §6,
// `[spec §2.7, §9]`): a fallback catch-all for any surface that doesn't already call
// `e.preventDefault()` itself (several already do, for their own custom menus — this is a harmless
// no-op there). Registered once, globally, rather than per-component.
document.addEventListener("contextmenu", (e) => e.preventDefault());

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
