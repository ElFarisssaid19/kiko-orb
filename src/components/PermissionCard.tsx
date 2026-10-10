import { useKikoStore } from "../store/useKikoStore";
import type { AgentKind, PermissionRequest } from "../types/events";
import { BUTTON } from "./buttons";

export const AGENT_NAMES: Record<AgentKind, string> = { "claude-code": "Claude Code" };

interface PermissionCardProps {
  request: PermissionRequest;
  agent: AgentKind;
  project?: string;
}

/** The agent waits for approval to run a command or edit a file. For now a decision only updates the store. */
export function PermissionCard({ request, agent, project }: PermissionCardProps) {
  const resolvePermission = useKikoStore((s) => s.resolvePermission);
  const isCommand = "command" in request;

  return (
    <section aria-label="Permission request" className="rounded-2xl border border-needs-you/40 bg-bg p-3">
      <p className="text-sm text-text">
        {AGENT_NAMES[agent]} wants to {isCommand ? "run" : "edit"}
        {project && <span className="text-text-2"> · {project}</span>}
      </p>
      <pre className="mt-2 overflow-x-auto rounded-lg border border-border bg-island px-3 py-2 font-mono text-[13px] leading-5 text-text">
        <code>{isCommand ? request.command : request.file}</code>
      </pre>
      <div className="mt-3 flex justify-end gap-2">
        <button type="button" className={BUTTON.ghost} onClick={() => resolvePermission("deny")}>
          Deny
        </button>
        <button type="button" className={BUTTON.secondary} onClick={() => resolvePermission("always")}>
          Always
        </button>
        <button type="button" className={BUTTON.primary} onClick={() => resolvePermission("allow")}>
          Allow
        </button>
      </div>
    </section>
  );
}
