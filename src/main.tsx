import React, { Suspense, lazy } from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ClaudeCodeSource } from "./events/ClaudeCodeSource";
import { connectEventSource } from "./events/connect";
import { currentPage, getHooksStatus, onHooksChanged } from "./native";
import { useKikoStore } from "./store/useKikoStore";
import "./styles/global.css";

const page = currentPage();

const SettingsPanel = lazy(() => import("./components/SettingsPanel").then((m) => ({ default: m.SettingsPanel })));
// Debug page for dev builds only: production builds drop it.
const Playground =
  import.meta.env.DEV && page === "playground" ? lazy(() => import("./playground/Playground")) : null;

if (page !== "settings") {
  connectEventSource(new ClaudeCodeSource());
  const refreshHooksStatus = () =>
    getHooksStatus()
      .then(useKikoStore.getState().setHooksStatus)
      .catch((error) => console.error("Kiko could not read the hooks status:", error));
  void refreshHooksStatus();
  void onHooksChanged(refreshHooksStatus);
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {page === "settings" ? (
      <Suspense fallback={null}>
        <SettingsPanel />
      </Suspense>
    ) : Playground ? (
      <Suspense fallback={null}>
        <Playground />
      </Suspense>
    ) : (
      <App />
    )}
  </React.StrictMode>,
);
