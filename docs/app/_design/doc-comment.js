// @flow
//
// A doc comment's text as blocks: paragraphs, fenced code and bulleted lists.
//
// The comments `uf doc` hands the API reference are Markdown in practice, and
// those three are all of it that the packages use. Anything else stays as its
// text. A module of its own so it can be tested without the glob that loads
// the reference's data, which only a bundler can evaluate.

export type Block =
  | {| readonly kind: "code", readonly text: string |}
  | {| readonly kind: "list", readonly items: $ReadOnlyArray<string> |}
  | {| readonly kind: "paragraph", readonly text: string |};

/** A comment's text as paragraphs, fenced code and bulleted lists. */
export function blocksOf(text: string): Array<Block> {
  const out: Array<Block> = [];
  const lines = text.split("\n");
  let index = 0;
  while (index < lines.length) {
    const line = lines[index];
    if (line.trim() === "") {
      index += 1;
      continue;
    }
    const fence = line.match(/^\s*(```|~~~)/);
    if (fence != null) {
      const body: Array<string> = [];
      index += 1;
      while (index < lines.length && !lines[index].trim().startsWith(fence[1])) {
        body.push(lines[index]);
        index += 1;
      }
      index += 1;
      out.push({ kind: "code", text: body.join("\n") });
      continue;
    }
    if (/^\s*[-*]\s+/.test(line)) {
      const items: Array<string> = [];
      while (index < lines.length && lines[index].trim() !== "") {
        const item = lines[index].match(/^\s*[-*]\s+(.*)$/);
        if (item != null) {
          items.push(item[1]);
        } else if (items.length > 0) {
          items[items.length - 1] = `${items[items.length - 1]} ${lines[index].trim()}`;
        }
        index += 1;
      }
      out.push({ kind: "list", items });
      continue;
    }
    const body: Array<string> = [];
    while (
      index < lines.length &&
      lines[index].trim() !== "" &&
      !/^\s*(```|~~~)/.test(lines[index]) &&
      !/^\s*[-*]\s+/.test(lines[index])
    ) {
      body.push(lines[index].trim());
      index += 1;
    }
    out.push({ kind: "paragraph", text: body.join(" ") });
  }
  return out;
}
