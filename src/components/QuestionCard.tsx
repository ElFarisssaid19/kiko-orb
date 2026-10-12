import { useState } from "react";
import { useKikoStore } from "../store/useKikoStore";
import type { QuestionRequest } from "../types/events";
import { BUTTON } from "./buttons";

/**
 * An AskUserQuestion call (one to four questions). Submit sends the choices to Claude Code;
 * "Reply in terminal" lets Claude Code ask there instead. Mount with `key` = the request id.
 */
export function QuestionCard({ request, project }: { request: QuestionRequest; project?: string }) {
  const answer = useKikoStore((s) => s.answer);
  /** Question text → chosen labels. */
  const [chosen, setChosen] = useState<Record<string, string[]>>({});

  const toggle = (question: string, label: string, multiSelect: boolean) =>
    setChosen((current) => {
      const picked = current[question] ?? [];
      const next = multiSelect
        ? picked.includes(label)
          ? picked.filter((l) => l !== label)
          : [...picked, label]
        : [label];
      return { ...current, [question]: next };
    });

  const complete = request.questions.every((q) => (chosen[q.question] ?? []).length > 0);
  const submit = () =>
    answer(request.id, {
      kind: "question",
      answers: Object.fromEntries(request.questions.map((q) => [q.question, (chosen[q.question] ?? []).join(", ")])),
    });

  return (
    <section aria-label="Question" className="rounded-2xl border border-needs-you/40 bg-bg p-3">
      <p className="font-mono text-[11px] text-text-2">
        Claude Code asks{project && ` · ${project}`}
      </p>

      {request.questions.map((q) => (
        <div key={q.question} className="mt-2">
          <p className="text-sm text-text">
            {q.header && <span className="mr-1.5 font-mono text-[11px] text-needs-you uppercase">{q.header}</span>}
            {q.question}
          </p>
          <p className="mt-0.5 font-mono text-[11px] text-text-2">{q.multiSelect ? "Pick any" : "Pick one"}</p>
          <div role="group" aria-label={q.question} className="mt-1.5 flex flex-col gap-1.5">
            {q.options.map((option) => {
              const isSelected = (chosen[q.question] ?? []).includes(option.label);
              return (
                <button
                  key={option.label}
                  type="button"
                  aria-pressed={isSelected}
                  onClick={() => toggle(q.question, option.label, q.multiSelect)}
                  className={`${BUTTON.option} ${
                    isSelected ? "border-needs-you bg-needs-you/10 text-text" : "border-border text-text-2 hover:text-text"
                  }`}
                >
                  <span
                    aria-hidden="true"
                    className={`size-3 shrink-0 border ${q.multiSelect ? "rounded-[3px]" : "rounded-full"} ${
                      isSelected ? "border-needs-you bg-needs-you" : "border-text-2"
                    }`}
                  />
                  <span className="min-w-0">
                    <span className="block">{option.label}</span>
                    {option.description && <span className="block text-xs text-text-2">{option.description}</span>}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      ))}

      <div className="mt-3 flex items-center justify-between">
        <button type="button" className={BUTTON.text} onClick={() => answer(request.id, { kind: "release" })}>
          Reply in terminal
        </button>
        <button type="button" className={BUTTON.primary} disabled={!complete} onClick={submit}>
          Submit
        </button>
      </div>
    </section>
  );
}
