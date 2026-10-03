// @flow
//
// A StyleX stylesheet import has to follow `"use client"`. Ahead of it, the
// directive is an ordinary string and the RSC graph renders the module.

import { describe, expect, it } from "@uniflowed/test";
import { parseAst } from "vite";

import {
  insertAfterDirectivePrologue,
  shiftSourceMap,
} from "./internal/directive-prologue.js";
import { styleModuleSource } from "./internal/style-module.js";
import { opensWithUseClient } from "./internal/flight.js";

function inserted(source: string): string {
  return insertAfterDirectivePrologue(source, 'import "uf-style:app/note.js.css";').code;
}

describe("a stylesheet import after the directive prologue", () => {
  it("leaves use client as the module's first statement", () => {
    const code = inserted('"use client";\nexport function Note() { return null; }\n');
    expect(opensWithUseClient(parseAst(code))).toBe(true);
    expect(code).toBe(
      '"use client";\nimport "uf-style:app/note.js.css";\nexport function Note() { return null; }\n',
    );
  });

  it("keeps use strict ahead of use client", () => {
    const code = inserted('"use strict";\n"use client";\nexport function Note() { return null; }\n');
    expect(opensWithUseClient(parseAst(code))).toBe(true);
    expect(code.indexOf('"use strict"')).toBeLessThan(code.indexOf('"use client"'));
    expect(code.indexOf('"use client"')).toBeLessThan(code.indexOf("import "));
  });

  it("follows a hashbang, a flow header, and a server directive", () => {
    const code = inserted('#!/usr/bin/env node\n// @flow\n"use server";\nexport function save() {}\n');
    expect(code.startsWith("#!/usr/bin/env node\n")).toBe(true);
    expect(code.indexOf('"use server"')).toBeLessThan(code.indexOf("import "));
    expect(code.indexOf("import ")).toBeLessThan(code.indexOf("export function save"));
  });

  it("does not treat an expression that mentions the words as a directive", () => {
    const code = inserted('"use client".length;\n');
    expect(code.startsWith('import "uf-style:app/note.js.css";\n')).toBe(true);
    expect(opensWithUseClient(parseAst(code))).toBe(false);
  });

  it("serves a stylesheet link the CSS that was compiled without the direct query", () => {
    const styles = new Map([["uf-style:app/note.js.css", ".x{color:red}"]]);
    expect(styleModuleSource(styles, "uf-style:app/note.js.css?direct")).toBe(".x{color:red}");
    expect(styleModuleSource(styles, "uf-style:app/note.js.css")).toBe(".x{color:red}");
  });

  it("shifts the source map at the line the import occupies", () => {
    const map = { version: 3, mappings: "AAAA;AACA;AACA", sources: ["note.js"] };
    const shifted = shiftSourceMap(map, 1, 1);
    expect(shifted.mappings).toBe("AAAA;;AACA;AACA");
  });
});
