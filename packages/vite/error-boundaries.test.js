// @flow
//
// An `$error.js` without `"use client"` fails the route table, not the first
// page that throws.
//
// Before this, such a project built and deployed, and `@uniflowed/router`
// refused the boundary only when a page threw: the visitor got a bare text
// `500`, the reason went to the server log, and the routing guide's own
// `$error.js` example had no directive. Found by the deploy matrix
// (ubugeeei-prod/uf#1478), whose fixture's boundary was written from that
// example.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { afterAll, describe, expect, it } from "@uniflowed/test";

import uniflowed from "./index.js";
import {
  errorBoundaryModules,
  hasUseClientDirective,
  refuseServerErrorBoundaries,
} from "./internal/error-boundaries.js";
import { scanRoutes } from "./internal/routes.js";

const roots: Array<string> = [];

afterAll(() => {
  for (const root of roots) fs.rmSync(root, { recursive: true, force: true });
});

/** A project with a page, a layout and `app/boom/$error.js` opening with `boundary`. */
function project(boundary: string): string {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "uf-error-boundary-")));
  roots.push(root);
  const write = (file: string, source: string) => {
    fs.mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
    fs.writeFileSync(path.join(root, file), source);
  };
  write("app/$layout.js", "export default function Layout({ children }) { return children; }\n");
  write("app/$page.js", "export default function Home() { return null; }\n");
  write("app/boom/[id]/$page.js", "export default function Boom() { throw new Error('boom'); }\n");
  write(
    "app/boom/$error.js",
    `${boundary}\nexport default function Boundary({ error }) { return error.kind; }\n`,
  );
  return root;
}

describe("hasUseClientDirective", () => {
  it("finds the directive after comments and other directives", () => {
    for (const source of [
      '"use client";\nexport default 1;',
      "'use client'\nexport default 1;",
      '// @flow\n"use client";\n',
      '"use client";\n// @flow\n',
      '/* a licence */\n// @flow\n"use client"',
      '"use strict";\n"use client";\n',
      `${String.fromCharCode(0xfeff)}"use client";`,
    ]) {
      expect(hasUseClientDirective(source)).toBe(true);
    }
  });

  it("does not find it where it is not a directive", () => {
    for (const source of [
      "",
      "// @flow\nimport * as React from 'react';\n\"use client\";",
      '"use server";\n',
      '"use client".length;\n',
      "const directive = 'use client';\n",
      "/* never closed",
    ]) {
      expect(hasUseClientDirective(source)).toBe(false);
    }
  });
});

describe("refuseServerErrorBoundaries", () => {
  it("names a boundary without the directive", () => {
    const root = project("// @flow");
    const table = scanRoutes(path.join(root, "app"));
    expect(errorBoundaryModules(table)).toEqual([path.join(root, "app/boom/$error.js")]);
    expect(() => refuseServerErrorBoundaries(table, root)).toThrow(
      "not a client module: app/boom/$error.js. An `$error.js` catches a throw while the browser renders",
    );
  });

  it("accepts a boundary that opens with the directive", () => {
    const root = project('// @flow\n"use client";');
    expect(() =>
      refuseServerErrorBoundaries(scanRoutes(path.join(root, "app")), root),
    ).not.toThrow();
  });
});

describe("the route table the rsc graph loads", () => {
  /** `virtual:uf/routes` as the rsc environment of a build asks for it. */
  const load = (root: string): mixed => {
    const plugin: $FlowFixMe = uniflowed({ root, config: {} })[0];
    plugin.configResolved({ root, base: "/", command: "build", isProduction: true });
    return plugin.load.call({ environment: { name: "rsc" } }, "\0virtual:uf/routes", { ssr: true });
  };

  it("fails the build on a server error boundary, naming the file", () => {
    expect(() => load(project("// @flow"))).toThrow("app/boom/$error.js");
  });

  it("builds with the directive", () => {
    expect(typeof load(project('"use client";\n// @flow'))).toBe("string");
  });
});
