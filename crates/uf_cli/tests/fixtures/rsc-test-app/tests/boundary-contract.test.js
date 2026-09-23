// @flow
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { afterAll, expect, it, uft } from "@uniflowed/test";
import { buildApp } from "@uniflowed/router/testing";

// `docs/app/guide/server-components/$page.mdx`: "`$error.js` is a client
// module. […] a boundary module without `"use client"` is refused with its file
// named." (ubugeeei-prod/uf#1501)
//
// A copy of this project with the directive taken off `app/notes/$error.js`,
// built for real, because the refusal is the rsc graph's and only a build has
// one. The copy lives under `.uf/`, inside the project, so its packages resolve
// from the project's own `node_modules`.

const PROJECT = path.resolve(fileURLToPath(String(import.meta.url)), "../..");
const COPY = path.join(PROJECT, ".uf", "claims", "error-without-directive");

/** Copy a file or a directory tree; `fs.cpSync` has no Flow declaration here. */
function copy(from: string, to: string): void {
  if (fs.statSync(from).isDirectory()) {
    fs.mkdirSync(to, { recursive: true });
    for (const name of fs.readdirSync(from)) copy(path.join(from, name), path.join(to, name));
  } else {
    fs.copyFileSync(from, to);
  }
}

afterAll(() => {
  fs.rmSync(COPY, { recursive: true, force: true });
});

it(
  "refuses an error boundary without the use client directive, naming its file",
  async () => {
    fs.rmSync(COPY, { recursive: true, force: true });
    fs.mkdirSync(COPY, { recursive: true });
    for (const entry of ["app", "app.js", "uf.config.js", "package.json"]) {
      copy(path.join(PROJECT, entry), path.join(COPY, entry));
    }
    const boundary = path.join(COPY, "app", "notes", "$error.js");
    const source = fs.readFileSync(boundary, "utf8");
    fs.writeFileSync(boundary, source.replace(/^"use client";\n/, ""));

    const app = await buildApp({ root: COPY });
    // Whatever the host reports through: the console, or the process logger's
    // own writes.
    const logged = [
      uft.spyOn(console, "error").mockImplementation(() => {}),
      uft.spyOn(process.stderr, "write").mockImplementation(() => true),
      uft.spyOn(process.stdout, "write").mockImplementation(() => true),
    ];
    try {
      const response = await app.render("/notes/1");
      const html = await response.text();
      expect(response.status).toBe(500);
      const said = [
        ...app.errors(),
        ...logged.flatMap((spy) => spy.mock.calls.flatMap((call) => call.args)),
      ]
        .map((value) => (value instanceof Error ? value.message : String(value)))
        .join("\n");
      expect(said).toContain("app/notes/$error.js");
      expect(said).toContain("use client directive");
      // The reason is the operator's, not the visitor's.
      expect(html).not.toContain("use client directive");
    } finally {
      for (const spy of logged) spy.mockRestore();
    }
  },
  { timeout: 120_000 },
);
