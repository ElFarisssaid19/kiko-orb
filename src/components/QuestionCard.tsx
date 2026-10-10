import { useState } from "react";
import { useKikoStore } from "../store/useKikoStore";
import type { QuestionRequest } from "../types/events";
import { BUTTON } from "./buttons";

/** The agent asks the user to pick among options. Mount with `key` = the question so the selection resets. */
export function QuestionCard({ request }: { request: QuestionRequest }) {
  const answerQuestion = useKikoStore((s) => s.answerQuestion);
  const [selected, setSelected] = useState<string[]>([]);
  const { multiSelect } = request;

  const toggle = (option: string) =>
    setSelected((current) => {
      if (!multiSelect) return [option];
      return current.includes(option) ? current.filter((o) => o !== option) : [...current, option];
    });

  return (
    <section aria-label="Question" className="rounded-2xl border border-needs-you/40 bg-bg p-3">
      <p className="text-sm text-text">{request.question}</p>
      <p className="mt-0.5 font-mono text-[11px] text-text-2">{multiSelect ? "Pick any" : "Pick one"}</p>

      <div role="group" aria-label="Options" className="mt-2 flex flex-col gap-1.5">
        {request.options.map((option) => {
          const isSelected = selected.includes(option);
          return (
            <button
              key={option}
              type="button"
              aria-pressed={isSelected}
              onClick={() => toggle(option)}
              className={`${BUTTON.option} ${
                isSelected ? "border-needs-you bg-needs-you/10 text-text" : "border-border text-text-2 hover:text-text"
              }`}
            >
              <span
                aria-hidden="true"
                className={`size-3 shrink-0 border ${multiSelect ? "rounded-[3px]" : "rounded-full"} ${
                  isSelected ? "border-needs-you bg-needs-you" : "border-text-2"
                }`}
              />
              {option}
            </button>
          );
        })}
      </div>

      <div className="mt-3 flex items-center justify-between">
        <button type="button" className={BUTTON.text} onClick={() => answerQuestion("terminal")}>
          Reply in terminal
        </button>
        <button
          type="button"
          className={BUTTON.primary}
          disabled={selected.length === 0}
          onClick={() => answerQuestion(selected)}
        >
          Submit
        </button>
      </div>
    </section>
  );
}
