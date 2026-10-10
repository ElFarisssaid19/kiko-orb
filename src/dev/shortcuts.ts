// Dev-only keyboard shortcuts to drive the island on the real window (click the island first so it has focus).
//   p / q          request the fixture permission / ask the fixture question
//   Shift+P / Q    same after 3s: click another window meanwhile to test the island while unfocused
//   h / c / e      island hidden / compact / expanded
//   f / 0          reload the fixtures / clear all sessions
import { useKikoStore } from "../store/useKikoStore";
import { fixturePermission, fixtureQuestion, loadFixtures } from "./fixtures";

const DELAY_MS = 3000;

export function installDevShortcuts() {
  window.addEventListener("keydown", (event) => {
    if (event.ctrlKey || event.metaKey || event.altKey || event.target instanceof HTMLInputElement) return;
    const store = useKikoStore.getState();
    const later = (action: () => void) => (event.shiftKey ? setTimeout(action, DELAY_MS) : action());
    switch (event.key.toLowerCase()) {
      case "p":
        return later(() => useKikoStore.getState().requestPermission(fixturePermission));
      case "q":
        return later(() => useKikoStore.getState().askQuestion(fixtureQuestion));
      case "h":
        return store.setIslandState("hidden");
      case "c":
        return store.setIslandState("compact");
      case "e":
        return store.setIslandState("expanded");
      case "f":
        return loadFixtures();
      case "0":
        return store.reset();
    }
  });
}
