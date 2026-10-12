import { useKikoStore } from "../store/useKikoStore";
import type { AgentKind, PermissionDecision, PermissionRequest } from "../types/events";
import { BUTTON } from "./buttons";

export const AGENT_NAMES: Record<AgentKind, string> = { "claude-code": "Claude Code" };

interface PermissionCardProps {
  request: PermissionRequest;
  agent: AgentKind;
  project?: string;
}

/** The agent waits for approval. The decision (or "reply in terminal") goes back to Claude Code through the event source. */
export function PermissionCard({ request, agent, project }: PermissionCardProps) {
  const answer = useKikoStore((s) => s.answer);
  const decide = (decision: PermissionDecision) => answer(request.id, { kind: "permission", decision });
  const verb = request.kind === "command" ? "run" : request.kind === "file" ? "edit" : `use ${request.tool}`;

  return (
    <section aria-label="Permission request" className="rounded-2xl border border-needs-you/40 bg-bg p-3">
      <p className="text-sm text-text">
        {AGENT_NAMES[agent]} wants to {verb}
        {project && <span className="text-text-2"> · {project}</span>}
      </p>
      <pre className="mt-2 overflow-x-auto rounded-lg border border-border bg-island px-3 py-2 font-mono text-[13px] leading-5 text-text">
        <code>{request.subject}</code>
      </pre>
      <div className="mt-3 flex items-center gap-2">
        {/* No decision: Claude Code shows its own prompt in the terminal. */}
        <button
          type="button"
          className={`${BUTTON.text} mr-auto`}
          onClick={() => answer(request.id, { kind: "release" })}
        >
          Reply in terminal
        </button>
        <button type="button" className={BUTTON.ghost} onClick={() => decide("deny")}>
          Deny
        </button>
        {/* Only when Claude Code offers a rule to remember: "Always" sends that rule back. */}
        {request.canAlwaysAllow && (
          <button type="button" className={BUTTON.secondary} onClick={() => decide("always")}>
            Always
          </button>
        )}
        <button type="button" className={BUTTON.primary} onClick={() => decide("allow")}>
          Allow
        </button>
      </div>
    </section>
  );
}
