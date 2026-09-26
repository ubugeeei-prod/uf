// @flow
//
// Where `describe` and `it` say they were written.
//
// Registration asks the stack for the caller's position once per case. On Node
// it reads V8's call sites and maps the one frame it keeps through its module's
// source map (`internal/frames.js`'s `callerSite`), rather than printing the
// whole source-mapped `.stack` and parsing a line back out of it. The two must
// agree to the column, or a failure would name a place the author did not
// write — so these tests take the same stack both ways and compare.

import { describe, expect, it } from "@uniflowed/test";

import { type Site, callerSite, firstUserSite } from "./internal/frames.js";
import { it as registerCase, reset } from "./internal/registry.js";
import { type Result, run as runRegistered } from "./internal/run.js";

type Both = {| readonly structured: Site | null | void, readonly printed: Site | null |};

/** The caller's position, read from call sites and from the printed stack. */
function probe(): Both {
  const structured = callerSite(probe);
  const holder: $FlowFixMe = {};
  Error.captureStackTrace(holder, probe);
  return { structured, printed: firstUserSite(holder.stack) };
}

/** Whether this is the host the structured path is for. */
function onNode(): boolean {
  const host: $FlowFixMe = globalThis;
  return (
    typeof host.process?.versions?.node === "string" &&
    host.process.versions.bun == null &&
    host.Deno == null
  );
}

describe("the registration site", () => {
  it("is the position the printed stack names", () => {
    // The annotation before the call is stripped by the transform, so the
    // generated column differs from the written one: an answer that skipped
    // the source map would be off by its width.
    const site: {| readonly structured: Site | null | void, readonly printed: Site | null |} =
      probe();
    if (!onNode()) {
      // Bun and Deno keep reading the printed stack; see `callerSite`.
      expect(site.structured).toBe(undefined);
      return;
    }
    expect(site.printed).not.toBe(null);
    expect(site.structured).toEqual(site.printed);
  });

  it("is the generated position when Node maps nothing", () => {
    const host: $FlowFixMe = globalThis;
    if (!onNode() || host.process.sourceMapsEnabled !== true) {
      return;
    }
    host.process.setSourceMapsEnabled(false);
    try {
      const site: {| readonly structured: Site | null | void, readonly printed: Site | null |} =
        probe();
      expect(site.structured).toEqual(site.printed);
    } finally {
      host.process.setSourceMapsEnabled(true);
    }
  });

  it("leaves a prepareStackTrace somebody installed where it was", () => {
    const errors: $FlowFixMe = Error;
    const before = errors.prepareStackTrace;
    probe();
    expect(errors.prepareStackTrace).toBe(before);
  });

  it("is what a registered case reports, through every wrapper", async () => {
    const results: Array<Result> = [];
    reset();
    const above = probe().printed;
    registerCase("plain", () => {});
    registerCase.each([1])("row %s", () => {});
    registerCase.skipBecause("reasoned", "a reason");
    await runRegistered({ file: "virtual.test.js" }, (result) => {
      results.push(result);
    });
    reset();
    expect(above).not.toBe(null);
    const line = above?.line ?? 0;
    expect(results.map((result) => result.line)).toEqual([line + 1, line + 2, line + 3]);
  });
});
