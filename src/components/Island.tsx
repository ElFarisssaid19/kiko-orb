import type { ReactNode } from "react";

/** Placeholder for the floating island container. */
export function Island({ children }: { children: ReactNode }) {
  return (
    <div className="rounded-full border border-border bg-island px-6 py-3">
      {children}
    </div>
  );
}
