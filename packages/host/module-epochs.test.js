// @flow
//
// What a kept `uf test --watch` worker loads afresh after an edit, and what it
// refuses to promise. `./internal/module-epochs.js` says why each rule is
// there; the worker that asks is `../test/worker.js`, and
// `../test/kept-worker.test.js` drives a real one through an edit.

import { describe, expect, it } from "@uniflowed/test";

import { createModuleEpochs } from "./internal/module-epochs.js";

const url = (path: string): string => `file://${path}`;

describe("module epochs", () => {
  it("answers every import as it was until something changes", () => {
    const epochs = createModuleEpochs();

    expect(epochs.resolved(url("/p/a.test.js?uf-run=1"), url("/p/dep.js"), true)).toBe(
      url("/p/dep.js"),
    );
    expect(epochs.epochOf("/p/dep.js")).toBe(0);
  });

  it("loads a changed module, and each module that reaches it, under a new URL", () => {
    const epochs = createModuleEpochs();
    epochs.resolved(url("/p/a.test.js?uf-run=1"), url("/p/mid.js"), true);
    epochs.resolved(url("/p/mid.js"), url("/p/dep.js"), true);
    epochs.resolved(url("/p/a.test.js?uf-run=1"), url("/p/other.js"), true);

    expect(epochs.invalidate(["/p/dep.js"])).toBe(true);

    // The edited module and the module between it and the test file.
    expect(epochs.resolved(url("/p/mid.js"), url("/p/dep.js"), true)).toBe(
      url("/p/dep.js?uf-epoch=1"),
    );
    expect(epochs.resolved(url("/p/a.test.js?uf-run=2"), url("/p/mid.js"), true)).toBe(
      url("/p/mid.js?uf-epoch=1"),
    );
    // A module no edit reaches keeps the instance the worker already has.
    expect(epochs.resolved(url("/p/a.test.js?uf-run=2"), url("/p/other.js"), true)).toBe(
      url("/p/other.js"),
    );
  });

  it("gives each later edit an epoch of its own", () => {
    const epochs = createModuleEpochs();
    epochs.resolved(url("/p/a.test.js"), url("/p/dep.js"), true);

    epochs.invalidate(["/p/dep.js"]);
    epochs.invalidate(["/p/dep.js"]);

    expect(epochs.epochOf("/p/dep.js")).toBe(2);
    expect(epochs.resolved(url("/p/a.test.js"), url("/p/dep.js"), true)).toBe(
      url("/p/dep.js?uf-epoch=2"),
    );
  });

  it("follows an edge through the instance an earlier epoch loaded", () => {
    // `mid.js?uf-epoch=1` imports `dep.js`: the parent's query names an
    // instance, not a different file, so the edge is still mid -> dep.
    const epochs = createModuleEpochs();
    epochs.resolved(url("/p/a.test.js"), url("/p/mid.js"), true);
    epochs.invalidate(["/p/mid.js"]);
    epochs.resolved(url("/p/mid.js?uf-epoch=1"), url("/p/dep.js"), true);

    epochs.invalidate(["/p/dep.js"]);

    expect(epochs.epochOf("/p/mid.js")).toBe(2);
  });

  it("leaves a URL that already names its instance alone", () => {
    const epochs = createModuleEpochs();
    epochs.resolved(url("/p/a.test.js"), url("/p/dep.js"), true);
    epochs.invalidate(["/p/dep.js"]);

    // A module mock's revision, and the test file's own run number.
    expect(epochs.resolved(url("/p/a.test.js"), url("/p/dep.js?uf-modules=1.1"), true)).toBe(
      url("/p/dep.js?uf-modules=1.1"),
    );
    expect(epochs.resolved(null, url("/p/dep.js?uf-run=3"), true)).toBe(url("/p/dep.js?uf-run=3"));
  });

  it("does not touch what is not a file", () => {
    const epochs = createModuleEpochs();
    epochs.invalidate(["/p/dep.js"]);

    expect(epochs.resolved(url("/p/a.test.js"), "node:fs", true)).toBe("node:fs");
    expect(epochs.resolved(url("/p/a.test.js"), "data:text/javascript,1", true)).toBe(
      "data:text/javascript,1",
    );
  });

  it("refuses an edit that reaches a module require() loaded, and changes nothing", () => {
    // The CommonJS cache is keyed by path, whatever URL is asked for, so a
    // module `require()` reached cannot be loaded afresh by this means.
    const epochs = createModuleEpochs();
    epochs.resolved(url("/p/a.test.js"), url("/p/legacy.cjs"), false);
    epochs.resolved(url("/p/legacy.cjs"), url("/p/dep.js"), false);

    expect(epochs.invalidate(["/p/dep.js"])).toBe(false);
    expect(epochs.epochOf("/p/dep.js")).toBe(0);
    expect(epochs.resolved(url("/p/a.test.js"), url("/p/legacy.cjs"), false)).toBe(
      url("/p/legacy.cjs"),
    );
  });

  it("says yes to an edit of a module nothing loaded yet, which loads fresh anyway", () => {
    const epochs = createModuleEpochs();

    expect(epochs.invalidate(["/p/never-loaded.js"])).toBe(true);
  });
});
