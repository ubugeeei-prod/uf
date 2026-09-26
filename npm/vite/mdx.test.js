// @flow
//
// `app.builtins.markdown.mdx`, read by the `uf:mdx` plugin. Each case compiles
// a one-line document through the plugin `uniflowed()` returns, so what is
// checked is the module MDX writes, not the options object it was given.

import { describe, expect, it } from "@uniflowed/test";
import * as React from "@uniflowed/react";
import { renderToStaticMarkup } from "react-dom/server";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import uniflowed from "./index.js";

/** The module `uf:mdx` compiles `source` to, for a project with `mdx`. */
async function compiled(
  mdx: { readonly jsxImportSource?: string },
  source: string = "# Hello\n",
  extension: string = "mdx",
): Promise<string> {
  const config = { app: { builtins: { markdown: { mdx } } } };
  const plugin = uniflowed({ root: "/project", config }).find((each) => each.name === "uf:mdx");
  const result = await plugin.transform(source, `/project/app/$page.${extension}`);
  return result.code;
}

describe("the JSX runtime compiled MDX imports", () => {
  it("is React's when a project names none", async () => {
    expect(await compiled({})).toContain('from "react/jsx-runtime"');
  });

  it("is the package `jsxImportSource` names", async () => {
    const code = await compiled({ jsxImportSource: "preact" });
    expect(code).toContain('from "preact/jsx-runtime"');
    expect(code).not.toContain('from "react/jsx-runtime"');
  });
});

describe("ox-content Markdown and MDX frontmatter", () => {
  it("renders GFM, frontmatter, exports and embedded JSX through React", async () => {
    const source = `---
title: Native Markdown
---

export const metadata = { title: "Native API" };
export const Note = ({ label, ...props }) => <aside {...props}>{label}</aside>;

# Native Markdown

| Field | Value |
| --- | --- |
| id | stable |

- [x] shipped

<Note label={"ready"} {...{id: "note"}} />

{/* a comment */}

{1 + 2}
`;
    const code = await compiled({}, source);
    const directory = mkdtempSync(path.join(tmpdir(), "uf-native-mdx-"));
    try {
      const runtime = pathToFileURL(
        createRequire(import.meta.url).resolve("react/jsx-runtime"),
      ).href;
      const file = path.join(directory, "page.mjs");
      writeFileSync(file, code.replaceAll('"react/jsx-runtime"', JSON.stringify(runtime)));
      // $FlowExpectedError[unsupported-syntax] The fixture module URL is generated at runtime.
      const page = await import(pathToFileURL(file).href);
      expect(page.frontmatter.title).toBe("Native Markdown");
      expect(page.metadata.title).toBe("Native API");
      const html = renderToStaticMarkup(<page.default />);
      expect(html).toContain('<h1 id="native-markdown">Native Markdown</h1>');
      expect(html).toContain("<table>");
      expect(html).toContain('checked=""');
      expect(html).toContain('<aside id="note">ready</aside>');
      expect(html).toContain("3");
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it("compiles native Markdown tables, frontmatter and literal HTML", async () => {
    const code = await compiled(
      {},
      "---\ntitle: Native\n---\n# Guide\n\n| Field | Value |\n| --- | --- |\n| id | stable |\n\n<script>alert(1)</script>\n",
      "md",
    );
    expect(code).toContain('"table"');
    expect(code).toContain('"title": "Native"');
    expect(code).toContain("<script>alert(1)</script>");
  });

  it("treats braces in a plain Markdown file as text", async () => {
    const code = await compiled({}, "# Guide\n\nUse {a + b} as text.\n", "md");
    expect(code).toContain("Use {a + b} as text.");
  });

  it("keeps prose continuations starting with import or export as Markdown", async () => {
    const code = await compiled(
      {},
      "A preceding paragraph line\nimport of that kind is `any`.\n\nAnother paragraph\nexport name to import.\n",
    );
    expect(code).toContain("import of that kind is ");
    expect(code).toContain("export name to import.");
  });

  it("fails compilation for invalid embedded JavaScript", async () => {
    await expect(compiled({}, "# Guide\n\n{value +}\n")).rejects.toThrow(
      "Could not parse expression",
    );
  });
});
