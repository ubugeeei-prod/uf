// @flow
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { it, expect } from "@uniflowed/test";
import { startDevState, recordDevEvent } from "./internal/dev-state.js";

it("bounds diagnostics, carries async request ids, and clears current errors after edits", async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-dev-state-"));
  const state = startDevState(root, () => ({ routes: [{ path: "/" }], actions: [] }));
  const read = () => JSON.parse(fs.readFileSync(path.join(root, ".uf/dev-state.json"), "utf8"));
  let id;
  try {
    await new Promise((resolve, reject) => {
      state.middleware(
        { url: "/page?secret=hidden", method: "GET" },
        {
          setHeader(_key, value) {
            id = value;
          },
        },
        () => {
          Promise.resolve()
            .then(() => {
              recordDevEvent({
                event: "diagnostic",
                severity: "error",
                message: "hydration mismatch",
              });
              resolve();
            })
            .catch(reject);
        },
      );
    });
    expect(read().errors[0].requestId).toBe(id);
    expect(read().errors[0].url).toBe("/page");
    expect(read().errors[0].kind).toBe("hydration");
    for (let i = 0; i < 170; i++)
      recordDevEvent({ event: "log", level: "info", message: "x".repeat(5000) });
    expect(read().logs.length).toBe(150);
    expect(read().logs[0].message.length).toBe(4000);
    recordDevEvent({ event: "source-changed" });
    expect(read().errors).toEqual([]);
    expect(read().generation).toBe(1);
  } finally {
    state.close();
    expect(fs.existsSync(path.join(root, ".uf/dev-state.json"))).toBe(false);
    fs.rmSync(root, { recursive: true, force: true });
  }
});
