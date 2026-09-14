// @flow
//
// The tags a served document links, as the build recorded them.
//
// `uf build` writes them beside the server bundle and every later server reads
// them back — `uf start`, `uf preview`, `--adapter` and `--compile` — because
// the client manifest alone no longer holds them: an application React Server
// Components render links the stylesheets its rsc graph emitted, which no
// import from the client entry reaches. See ubugeeei-prod/uf#519.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { afterAll, describe, expect, it } from "@uniflowed/test";

import { DOCUMENT_ASSETS_FILE, documentAssetsFor } from "./internal/serve.js";

const directories: Array<string> = [];

afterAll(() => {
  for (const directory of directories) {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

/** An empty server bundle directory. */
function serverDir(): string {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "uf-document-assets-"));
  directories.push(directory);
  return directory;
}

/** A client manifest whose entry carries one stylesheet. */
const manifest = {
  "\0virtual:uf/client": {
    file: "assets/client-1.js",
    isEntry: true,
    name: "client",
    css: ["assets/client-1.css"],
  },
};

describe("the tags a served document links", () => {
  it("are the ones the build recorded, including stylesheets the manifest cannot reach", async () => {
    const directory = serverDir();
    const recorded = {
      scripts: ["/assets/client-1.js"],
      styles: ["/assets/layout-2.css", "/assets/client-1.css"],
      preloads: [],
    };
    fs.writeFileSync(path.join(directory, DOCUMENT_ASSETS_FILE), JSON.stringify(recorded));

    expect(await documentAssetsFor(directory, manifest)).toEqual(recorded);
  });

  it("are the client manifest's, for a build that recorded none", async () => {
    expect(await documentAssetsFor(serverDir(), manifest)).toEqual({
      scripts: ["/assets/client-1.js"],
      styles: ["/assets/client-1.css"],
      preloads: [],
    });
  });

  it("name the file when what is there is not what a build writes", async () => {
    const directory = serverDir();
    fs.writeFileSync(path.join(directory, DOCUMENT_ASSETS_FILE), "{ not json");

    let thrown: mixed = null;
    try {
      await documentAssetsFor(directory, manifest);
    } catch (error) {
      thrown = error;
    }

    expect(String(thrown)).toContain(DOCUMENT_ASSETS_FILE);
  });
});
