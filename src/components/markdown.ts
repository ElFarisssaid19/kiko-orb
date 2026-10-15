// Just enough Markdown for Claude's chat replies: fenced code blocks, headings, paragraphs that keep
// their line breaks (lists read fine that way), inline `code` and **bold**. No HTML is ever produced:
// the chat view renders these blocks as React elements.

export type Inline = { kind: "text" | "code" | "bold"; text: string };

export type Block =
  | { kind: "code"; lang: string; code: string }
  | { kind: "heading"; inlines: Inline[] }
  | { kind: "text"; inlines: Inline[] };

const OPENING_FENCE = /^\s*(`{3,}|~{3,})\s*([^\s`]*)/;
const HEADING = /^\s{0,3}#{1,6}\s+(.*?)\s*#*\s*$/;
const INLINE = /(`+)([\s\S]*?[^`])\1(?!`)|\*\*(?=\S)([\s\S]*?\S)\*\*/g;

export function parseInline(text: string): Inline[] {
  const inlines: Inline[] = [];
  let last = 0;
  for (const match of text.matchAll(INLINE)) {
    if (match.index > last) inlines.push({ kind: "text", text: text.slice(last, match.index) });
    inlines.push(match[1] ? { kind: "code", text: match[2].trim() || match[2] } : { kind: "bold", text: match[3] });
    last = match.index + match[0].length;
  }
  if (last < text.length) inlines.push({ kind: "text", text: text.slice(last) });
  return inlines;
}

export function parseMarkdown(markdown: string): Block[] {
  const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
  const blocks: Block[] = [];
  let paragraph: string[] = [];
  const flush = () => {
    const text = paragraph.join("\n").replace(/^\n+|\s+$/g, "");
    if (text) blocks.push({ kind: "text", inlines: parseInline(text) });
    paragraph = [];
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const fence = OPENING_FENCE.exec(line);
    if (fence) {
      flush();
      // Closed by the same character, at least as many times; an unclosed block runs to the end.
      const closing = new RegExp(`^\\s*\\${fence[1][0]}{${fence[1].length},}\\s*$`);
      const code: string[] = [];
      while (++i < lines.length && !closing.test(lines[i])) code.push(lines[i]);
      blocks.push({ kind: "code", lang: fence[2], code: code.join("\n") });
      continue;
    }
    const heading = HEADING.exec(line);
    if (heading) {
      flush();
      blocks.push({ kind: "heading", inlines: parseInline(heading[1]) });
    } else if (line.trim() === "") {
      flush();
    } else {
      paragraph.push(line);
    }
  }
  flush();
  return blocks;
}
