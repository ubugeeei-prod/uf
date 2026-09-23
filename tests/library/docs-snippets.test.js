// @flow
//
// `tools/docs/snippets.js`, the check that every sample in the manual parses.
//
// The check runs in CI as `uf run docs:snippets` over the real pages with the
// real parser. These are the parts that decide *which* samples it reads and
// where it says a failure is, on inputs small enough to see.

import { describe, expect, it } from "@uniflowed/test";

import { asModule, failures, fences, isChecked } from "../../tools/docs/snippets.js";

const PAGE = [
  "# A page",
  "",
  "```js",
  "const a = 1;",
  "```",
  "",
  "~~~sh",
  "uf dev",
  "~~~",
  "",
  "```jsx fragment",
  "<A />",
  "<B />",
  "```",
  "",
  "````md",
  "```js",
  "not a fence of its own",
  "```",
  "````",
  "",
  "```ts",
  "const b: number = 2;",
  "```",
].join("\n");

describe("fences", () => {
  const found = fences("docs/app/x/$page.mdx", PAGE);

  it("reads every fence, with its language, its info string and its first line", () => {
    expect(found.map((fence) => [fence.lang, fence.meta, fence.line])).toEqual([
      ["js", "", 4],
      ["sh", "", 8],
      ["jsx", "fragment", 12],
      ["md", "", 17],
      ["ts", "", 23],
    ]);
  });

  it("keeps a shorter fence inside a longer one as the outer one's text", () => {
    expect(found[3].code).toBe("```js\nnot a fence of its own\n```");
  });

  it("checks JavaScript that is not marked as a fragment, and nothing else", () => {
    expect(found.filter(isChecked).map((fence) => fence.code)).toEqual(["const a = 1;"]);
  });
});

describe("asModule", () => {
  it("is Flow, and a module, with the sample starting on line 2", () => {
    const [first] = fences("p.mdx", PAGE);
    expect(asModule(first).split("\n").slice(0, 3)).toEqual([
      "// @flow",
      "const a = 1;",
      "export {};",
    ]);
  });
});

describe("failures", () => {
  const [first] = fences("docs/app/x/$page.mdx", "\n\n```js\nconst a = 1;\nconst = ;\n```\n");
  const files = new Map([["sample-0.js", first]]);

  it("maps a syntax diagnostic back to the page's line, once per sample", () => {
    expect(
      failures(
        {
          diagnostics: [
            {
              rule: "flow/syntax",
              path: "/tmp/x/sample-0.js",
              line: 3,
              message: "Unexpected token `=`",
            },
            { rule: "flow/syntax", path: "/tmp/x/sample-0.js", line: 3, message: "recovering" },
            {
              rule: "import/no-extraneous-dependencies",
              path: "sample-0.js",
              line: 2,
              message: "no",
            },
          ],
        },
        files,
      ),
    ).toEqual([{ page: "docs/app/x/$page.mdx", line: 5, message: "Unexpected token `=`" }]);
  });

  it("reports nothing for a report with no syntax diagnostics", () => {
    expect(failures({ diagnostics: [] }, files)).toEqual([]);
    expect(failures(null, files)).toEqual([]);
  });
});
