// @flow
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { afterAll, expect, it } from "@uniflowed/test";
import { buildApp } from "@uniflowed/router/testing";

// `docs/app/guide/server-components/$page.mdx`: "`$error.js` is a client
// module. […] a boundary module without `"use client"` is refused with its file
// named." (ubugeeei-prod/uf#1501)
//
// A copy of this project with the directive taken off `app/notes/$error.js`,
// built for real, because only a build decides it. The copy lives under `.uf/`, inside the project, so its packages resolve
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

    // The build refuses it, before anything can be served: the refusal names
    // the file and says what to add.
    const refused = buildApp({ root: COPY });
    await expect(refused).rejects.toThrow("app/notes/$error.js");
    await expect(refused).rejects.toThrow('"use client"');
  },
  { timeout: 120_000 },
);
