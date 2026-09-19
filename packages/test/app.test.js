// @flow
import { mkdtemp, writeFile, chmod, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { it, expect } from "./index.js";
import { createTestApp } from "./app.js";
import { compareScreenshot } from "./internal/browser/screenshots.js";

it(
  "keeps request deadlines when the caller also supplies a cancellation signal",
  async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "uf-test-app-deadline-"));
    const binary = path.join(root, "fake-uf.cjs");
    let app;
    try {
      await writeFile(
        binary,
        '#!/usr/bin/env node\nconst server=require("node:http").createServer(()=>{}); server.listen(0,"127.0.0.1",()=>console.log(JSON.stringify({event:"listening",local:["http://127.0.0.1:"+server.address().port+"/"]}))); process.stdin.resume(); process.stdin.on("end",()=>process.exit(0));\n',
      );
      await chmod(binary, 0o755);
      app = await createTestApp({ root, binary, timeoutMs: 2000 });
      const caller = new AbortController();
      await expect(app.fetch("/", { signal: caller.signal })).rejects.toThrow();
      expect(caller.signal.aborted).toBe(false);
      caller.abort();
      await expect(app.fetch("/", { signal: caller.signal })).rejects.toThrow();
    } finally {
      await app?.close();
      await rm(root, { recursive: true, force: true });
    }
  },
  { timeout: 10000 },
);

it("refuses snapshot names that could overwrite another snapshot's failure artifacts", async () => {
  for (const name of ["example.actual", "example.diff"]) {
    await expect(compareScreenshot("", name)).rejects.toThrow("simple file name");
  }
});
