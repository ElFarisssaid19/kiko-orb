import { setChatSender, setResponder, useKikoStore } from "../store/useKikoStore";
import type { EventSource } from "./EventSource";

/** How long Kiko shows "done" after a turn ends; matches Kiko's own done → idle. */
const DONE_MS = 4000;

let connected: EventSource | null = null;

/** The event source the app is connected to, for the playground's debug log. */
export function connectedSource(): EventSource | null {
  return connected;
}

/** Feeds a source's events into the store, and the user's answers and chat messages back to the source. */
export function connectEventSource(source: EventSource): () => void {
  const doneTimers = new Map<string, ReturnType<typeof setTimeout>>();
  const dispatch = useKikoStore.getState().dispatch;

  setResponder((requestId, response) => source.respond(requestId, response));
  setChatSender((sessionId, message) => source.sendChat(sessionId, message));
  const unsubscribe = source.subscribe((event) => {
    dispatch(event);
    if (event.type === "turn-stopped") {
      const sessionId = event.session.id;
      clearTimeout(doneTimers.get(sessionId));
      doneTimers.set(
        sessionId,
        setTimeout(() => {
          doneTimers.delete(sessionId);
          dispatch({ type: "done-expired", sessionId });
        }, DONE_MS),
      );
    }
  });
  connected = source;
  Promise.resolve(source.start()).catch((error) => console.error("Kiko could not start listening:", error));

  return () => {
    unsubscribe();
    source.stop();
    doneTimers.forEach(clearTimeout);
    setResponder(null);
    setChatSender(null);
    connected = null;
  };
}
