// @flow
//
// Whether a module is being compiled for `uf.config.js`, as the loader thread
// sees it.
//
// `@uniflowed/vite`'s driver runs the loader hooks on the thread `register()`
// starts, and that thread's `process.env` is a copy taken when it started. The
// config loader used to say "this is the config bootstrap" by setting a
// variable on the main thread, which the loader thread never saw: the modules
// `uf.config.js` imports were compiled by a `uf transform` that went looking
// for the config they were part of. See ubugeeei-prod/uf#1674.
//
// A worker is the same kind of thread with the same copy of the environment,
// so this asks one.

import { Worker } from "node:worker_threads";
import { describe, expect, it } from "@uniflowed/test";

import { bootstrappingConfig, configBootstrapFlag, isConfigBootstrap } from "./transform.js";

const TRANSFORM = new URL("./transform.js", import.meta.url).href;

/** A thread that adopted `flag` and answers `isConfigBootstrap()` on request. */
function loaderLikeThread(flag: Int32Array): Worker {
  return new Worker(
    `import { parentPort, workerData } from "node:worker_threads";
     const { isConfigBootstrap, shareConfigBootstrapFlag } = await import(${JSON.stringify(TRANSFORM)});
     shareConfigBootstrapFlag(workerData.flag);
     parentPort.on("message", () => parentPort.postMessage({
       shared: isConfigBootstrap(),
       variable: process.env.UF_TRANSFORM_BOOTSTRAP_CONFIG ?? null,
     }));`,
    { eval: true, workerData: { flag } },
  );
}

function ask(worker: Worker): Promise<{ shared: boolean, variable: ?string }> {
  return new Promise((resolve, reject) => {
    worker.once("message", resolve);
    worker.once("error", reject);
    worker.postMessage(null);
  });
}

describe("the config bootstrap", () => {
  it("reaches a thread that cannot see this thread's environment change", async () => {
    const worker = loaderLikeThread(configBootstrapFlag());
    try {
      expect((await ask(worker)).shared).toBe(false);

      const during = await bootstrappingConfig(async () => {
        expect(isConfigBootstrap()).toBe(true);
        return ask(worker);
      });
      // The variable is what the loader thread used to go by, and it is
      // still unset there: only the shared counter carries the answer.
      expect(during.variable).toBe(null);
      expect(during.shared).toBe(true);

      expect(isConfigBootstrap()).toBe(false);
      expect((await ask(worker)).shared).toBe(false);
    } finally {
      await worker.terminate();
    }
  });

  it("stays on until the last of two overlapping loads ends", async () => {
    let finishFirst = () => {};
    const first = bootstrappingConfig(
      () =>
        new Promise((resolve) => {
          finishFirst = resolve;
        }),
    );
    await bootstrappingConfig(async () => {});
    expect(isConfigBootstrap()).toBe(true);
    finishFirst();
    await first;
    expect(isConfigBootstrap()).toBe(false);
  });
});
