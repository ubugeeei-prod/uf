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
import {
  describe as registerSuite,
  it as registerCase,
  reset,
  setKnownSites,
} from "./internal/registry.js";
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

describe("a position uf already read off the source", () => {
  // `uf` sends the positions its discovery found with each request, and a
  // registration whose full name is among them takes that position instead of
  // capturing a stack: cheaper, and right where a source map is not. The
  // positions here are deliberately not where the calls are, which is how the
  // cases can tell which one was used.

  async function registered(body: () => void): Promise<Array<Result>> {
    const results: Array<Result> = [];
    reset();
    try {
      body();
      await runRegistered({ file: "virtual.test.js" }, (result) => {
        results.push(result);
      });
    } finally {
      reset();
      setKnownSites(null);
    }
    return results;
  }

  it("is taken for a name uf placed, suites and cases alike", async () => {
    setKnownSites({ outer: [40, 1], "outer > inner": [41, 3], plain: [50, 1] });
    const results = await registered(() => {
      registerSuite("outer", () => {
        registerCase("inner", () => {});
      });
      registerCase("plain", () => {});
    });

    expect(results.map((result) => [result.name, result.line, result.column])).toEqual([
      ["outer > inner", 41, 3],
      ["plain", 50, 1],
    ]);
  });

  it("reads the stack for a name uf did not place, or one with a modifier", async () => {
    // A name made at run time is not in the table, and a modifier is never
    // sent: the stack's column for `it.skip(` is at `skip`, and the table's
    // would be at `it`.
    setKnownSites({ "skipped by name": [60, 1] });
    const above = probe().printed;
    const results = await registered(() => {
      registerCase(`made at ${String(1 + 1)}`, () => {});
      registerCase.skip("skipped by name", () => {});
    });

    const line = above?.line ?? 0;
    expect(results.map((result) => result.line)).toEqual([line + 2, line + 3]);
  });

  it("falls back to the stack for anything it cannot read", async () => {
    setKnownSites({ plain: "line five", other: [1] });
    const above = probe().printed;
    const results = await registered(() => {
      registerCase("plain", () => {});
    });

    expect(results.map((result) => result.line)).toEqual([(above?.line ?? 0) + 2]);
  });
});
