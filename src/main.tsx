import React, { Suspense, lazy } from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles/global.css";

// Dev playground for the character and the island: open the app URL with `?playground`.
// Behind import.meta.env.DEV so production builds drop it, with the fixtures it uses.
const Playground =
  import.meta.env.DEV && new URLSearchParams(window.location.search).has("playground")
    ? lazy(() => import("./playground/Playground"))
    : null;

async function start() {
  if (import.meta.env.DEV) {
    // Fake sessions and keyboard shortcuts until real events arrive (see src/dev/).
    const [{ loadFixtures }, { installDevShortcuts }] = await Promise.all([
      import("./dev/fixtures"),
      import("./dev/shortcuts"),
    ]);
    loadFixtures();
    installDevShortcuts();
  }

  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      {Playground ? (
        <Suspense fallback={null}>
          <Playground />
        </Suspense>
      ) : (
        <App />
      )}
    </React.StrictMode>,
  );
}

void start();
