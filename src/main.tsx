import React, { Suspense, lazy } from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles/global.css";

// Dev playground for the character: open the app URL with `?playground`.
const Playground = lazy(() => import("./playground/Playground"));
const isPlayground = new URLSearchParams(window.location.search).has("playground");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {isPlayground ? (
      <Suspense fallback={null}>
        <Playground />
      </Suspense>
    ) : (
      <App />
    )}
  </React.StrictMode>,
);
