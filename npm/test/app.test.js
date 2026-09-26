// @flow
import { mkdtemp, readFile, writeFile, chmod, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { it, expect } from "./index.js";
import { createTestApp } from "./app.js";
import { compareScreenshot } from "./internal/browser/screenshots.js";

it("waits for an inherited driver stream and its cleanup after the CLI exits", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "uf-test-app-close-"));
  const binary = path.join(root, "fake-uf.cjs");
  const marker = path.join(root, "closed");
  let app;
  try {
    const driver =
      'process.stdin.resume(); process.stdin.on("end", () => setTimeout(() => { require("node:fs").writeFileSync(process.argv[1], "closed"); process.exit(0); }, 100));';
    await writeFile(
      binary,
      `#!/usr/bin/env node
const child = require("node:child_process").spawn(process.execPath, ["-e", ${JSON.stringify(driver)}, ${JSON.stringify(marker)}], {stdio: ["pipe", "ignore", "inherit"]});
console.log(JSON.stringify({event: "listening", local: ["http://127.0.0.1:1/"]}));
process.stdin.resume();
process.stdin.on("end", () => { child.stdin.end(); process.exit(0); });
`,
    );
    await chmod(binary, 0o755);
    app = await createTestApp({ root, binary, timeoutMs: 2000 });
    await app.close();
    expect(await readFile(marker, "utf8")).toBe("closed");
  } finally {
    await app?.close();
    await rm(root, { recursive: true, force: true });
  }
});

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
