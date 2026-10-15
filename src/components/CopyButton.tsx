import { useEffect, useState } from "react";
import { BUTTON } from "./buttons";

/** Copies `text`, with a fallback for webviews that don't expose the async clipboard API. */
async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const field = document.createElement("textarea");
    field.value = text;
    field.style.position = "fixed";
    field.style.opacity = "0";
    document.body.append(field);
    field.select();
    const copied = document.execCommand("copy");
    field.remove();
    if (!copied) throw new Error("The clipboard is not available.");
  }
}

/** "Copy", then "Copied" for a moment. */
export function CopyButton({ text, label = "Copy" }: { text: string; label?: string }) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");
  useEffect(() => {
    if (state === "idle") return;
    const timer = setTimeout(() => setState("idle"), 1500);
    return () => clearTimeout(timer);
  }, [state]);

  return (
    <button
      type="button"
      className={`${BUTTON.secondary} shrink-0`}
      onClick={() => copyText(text).then(() => setState("copied"), () => setState("failed"))}
    >
      <span aria-live="polite">{state === "copied" ? "Copied" : state === "failed" ? "Copy failed" : label}</span>
    </button>
  );
}
