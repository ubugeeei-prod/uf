// @flow
//
// Which URL a module a test file reaches is loaded under, so that every test
// file gets its own copy of the project's modules. `./internal/file-scope.js`
// says why each rule is there; `../test/file-scope.test.js` holds a real worker
// to the promise across two files.

import { afterEach, beforeEach, describe, expect, it } from "@uniflowed/test";

import { FILE_SCOPE, fileScoped } from "./internal/file-scope.js";

const TEST = "file:///p/app/cart.test.js?uf-run=7";

describe("a test file's own copy of the project's modules", () => {
  // The worker running this file has a scope of its own, which the next file
  // depends on: it is put back exactly as it was, not deleted.
  let running: ?PropertyDescriptor<mixed> = null;

  beforeEach(() => {
    running = Object.getOwnPropertyDescriptor(globalThis, FILE_SCOPE);
    Object.defineProperty(globalThis, FILE_SCOPE, {
      value: { shared: ["/p/node_modules/@uniflowed/test", "/p/npm/test"] },
      configurable: true,
    });
  });

  afterEach(() => {
    Reflect.deleteProperty(globalThis, FILE_SCOPE);
    if (running != null) Object.defineProperty(globalThis, FILE_SCOPE, running);
  });

  it("loads what the test file imports under the file's run", () => {
    expect(fileScoped(TEST, "file:///p/app/cart.js", true)).toBe("file:///p/app/cart.js?uf-file=7");
  });

  it("carries the run on to everything those modules import", () => {
    expect(fileScoped("file:///p/app/cart.js?uf-file=7", "file:///p/app/price.js", true)).toBe(
      "file:///p/app/price.js?uf-file=7",
    );
  });

  it("keeps a module a finished file reached in that file's run", () => {
    // A timer the file left behind imports something late: it belongs to the
    // file that scheduled it, not to the one running now.
    expect(fileScoped("file:///p/app/cart.js?uf-file=3", "file:///p/app/late.js", true)).toBe(
      "file:///p/app/late.js?uf-file=3",
    );
  });

  it("keeps a query the test asked for, and adds the run to it", () => {
    expect(fileScoped(TEST, "file:///p/app/cart.js?a-second-copy", true)).toBe(
      "file:///p/app/cart.js?a-second-copy&uf-file=7",
    );
    expect(fileScoped(TEST, "file:///p/app/cart.js?uf-file=2", true)).toBe(
      "file:///p/app/cart.js?uf-file=2",
    );
  });

  it("shares installed packages and the runner itself", () => {
    expect(fileScoped(TEST, "file:///p/node_modules/react/index.js", true)).toBe(
      "file:///p/node_modules/react/index.js",
    );
    expect(fileScoped(TEST, "file:///p/npm/test/index.js", true)).toBe(
      "file:///p/npm/test/index.js",
    );
    // A directory whose name only starts like the runner's is not the runner.
    expect(fileScoped(TEST, "file:///p/npm/testing/index.js", true)).toBe(
      "file:///p/npm/testing/index.js?uf-file=7",
    );
  });

  it("leaves alone what a shared module or the worker imports", () => {
    expect(fileScoped("file:///p/npm/test/worker.js", "file:///p/app/x.js", true)).toBe(
      "file:///p/app/x.js",
    );
    expect(fileScoped(undefined, "file:///p/app/x.js", true)).toBe("file:///p/app/x.js");
  });

  it("never rewrites a require(), which the CommonJS cache answers by path", () => {
    expect(fileScoped(TEST, "file:///p/app/legacy.cjs", false)).toBe("file:///p/app/legacy.cjs");
  });

  it("does nothing outside a worker that asked for it", () => {
    Reflect.deleteProperty(globalThis, FILE_SCOPE);
    expect(fileScoped(TEST, "file:///p/app/cart.js", true)).toBe("file:///p/app/cart.js");
  });

  it("does not touch what is not a file", () => {
    expect(fileScoped(TEST, "node:fs", true)).toBe("node:fs");
  });
});
