// @flow
//
// `app.builtins.markdown.mdx`, read by the `uf:mdx` plugin. Each case compiles
// a one-line document through the plugin `uniflowed()` returns, so what is
// checked is the module MDX writes, not the options object it was given.

import { describe, expect, it } from "@uniflowed/test";
import uniflowed from "./index.js";

/** The module `uf:mdx` compiles `source` to, for a project with `mdx`. */
async function compiled(mdx: { readonly jsxImportSource?: string }): Promise<string> {
  const config = { app: { builtins: { markdown: { mdx } } } };
  const plugin = uniflowed({ root: "/project", config }).find((each) => each.name === "uf:mdx");
  const result = await plugin.transform("# Hello\n", "/project/app/$page.mdx");
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
