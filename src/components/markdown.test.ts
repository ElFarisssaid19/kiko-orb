import { describe, expect, it } from "vitest";
import { parseInline, parseMarkdown } from "./markdown";

describe("parseMarkdown", () => {
  it("splits a reply into paragraphs and fenced code blocks", () => {
    const reply = "All 12 tests pass.\n\nRun them again with:\n\n```sh\nnpm test\nnpm run build\n```\n\nDone.";
    expect(parseMarkdown(reply)).toEqual([
      { kind: "text", inlines: [{ kind: "text", text: "All 12 tests pass." }] },
      { kind: "text", inlines: [{ kind: "text", text: "Run them again with:" }] },
      { kind: "code", lang: "sh", code: "npm test\nnpm run build" },
      { kind: "text", inlines: [{ kind: "text", text: "Done." }] },
    ]);
  });

  it("keeps line breaks inside a paragraph, so lists read as written", () => {
    expect(parseMarkdown("Changed:\n- src/App.tsx\n- src/main.tsx")).toEqual([
      { kind: "text", inlines: [{ kind: "text", text: "Changed:\n- src/App.tsx\n- src/main.tsx" }] },
    ]);
  });

  it("keeps a code block's blank lines and indentation, and runs an unclosed one to the end", () => {
    expect(parseMarkdown("```ts\nfunction a() {\n\n  return 1;\n}\n```")).toEqual([
      { kind: "code", lang: "ts", code: "function a() {\n\n  return 1;\n}" },
    ]);
    expect(parseMarkdown("Here:\n~~~\nstill code")).toEqual([
      { kind: "text", inlines: [{ kind: "text", text: "Here:" }] },
      { kind: "code", lang: "", code: "still code" },
    ]);
  });

  it("only closes a fence with the same marker", () => {
    expect(parseMarkdown("````md\n```js\nx\n```\n````")).toEqual([{ kind: "code", lang: "md", code: "```js\nx\n```" }]);
  });

  it("reads headings", () => {
    expect(parseMarkdown("## Summary\nIt works.")).toEqual([
      { kind: "heading", inlines: [{ kind: "text", text: "Summary" }] },
      { kind: "text", inlines: [{ kind: "text", text: "It works." }] },
    ]);
  });
});

describe("parseInline", () => {
  it("finds inline code and bold", () => {
    expect(parseInline("Run `npm test` before **every** commit")).toEqual([
      { kind: "text", text: "Run " },
      { kind: "code", text: "npm test" },
      { kind: "text", text: " before " },
      { kind: "bold", text: "every" },
      { kind: "text", text: " commit" },
    ]);
  });

  it("leaves unmatched markers as text", () => {
    expect(parseInline("a ` b ** c")).toEqual([{ kind: "text", text: "a ` b ** c" }]);
    expect(parseInline("use ``a ` b`` here")).toEqual([
      { kind: "text", text: "use " },
      { kind: "code", text: "a ` b" },
      { kind: "text", text: " here" },
    ]);
  });
});
