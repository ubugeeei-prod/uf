// @flow
//
// The toolchain benchmark's arithmetic and its fixture, checked without a clock.
//
// Nothing here starts a server or times anything: a unit test that measured
// would be a flaky test, and measuring is what `uf run bench:toolchain` is for.
// What can be wrong without a clock is worth a test of its own — a median taken
// from the wrong middle; a fixture that stopped being deterministic, and so
// stopped being the same benchmark from one run to the next; a summary claiming
// tests the files do not have; identical files, which would flatter a
// content-addressed cache; and a work directory inside this repository, which
// the benchmark would generate into and then delete. And the comparison: each
// tool's copy has to carry the same tests as uf's, or the test rows compare
// different suites; and the regression gate's arithmetic has to fail on a
// real slowdown and nothing else.

import os from "node:os";
import path from "node:path";

import { describe, expect, it } from "@uniflowed/test";

import {
  HOT_FILE,
  HOT_MARKER,
  PRESETS,
  describeFixture,
  fixtureFiles,
  presetNamed,
  refuseInsideRepository,
} from "./fixture.js";
import { clientReferences, parseTimes, summarise } from "./measure.js";
import { compare, failures } from "./regress.js";
import { formatDuration } from "./report.js";
import { flowFiles, rivalFiles, testsPassed, versionIn } from "./rivals.js";
import type { RivalFixture } from "./rivals.js";

const VERSIONS = { uniflowed: "0.0.0-test", react: "19.3.0" };

describe("the toolchain benchmark's fixture", () => {
  it("is the same files every time it is generated", () => {
    const preset = presetNamed("small");
    expect(fixtureFiles(preset, VERSIONS)).toEqual(fixtureFiles(preset, VERSIONS));
  });

  it("has the routes, test files and tests its summary says it has", () => {
    for (const preset of PRESETS) {
      const files = fixtureFiles(preset, VERSIONS);
      const summary = describeFixture(preset, files);
      const paths = files.map((file) => file.path);
      const cases = files
        .map((file) => (file.contents.match(/^\s+it\(/gm) ?? []).length)
        .reduce((sum, found) => sum + found, 0);
      expect(paths.filter((file) => /^app\/r\d{3}\/\$page\.js$/.test(file)).length).toBe(
        preset.routes,
      );
      expect(paths.filter((file) => file.endsWith(".test.js")).length).toBe(summary.testFiles);
      expect(cases).toBe(summary.tests);
      expect(summary.files).toBe(files.length);
      expect(new Set(paths).size).toBe(files.length);
    }
  });

  it("never writes the same bytes into two files", () => {
    const files = fixtureFiles(presetNamed("large"), VERSIONS);
    expect(new Set(files.map((file) => file.contents)).size).toBe(files.length);
  });

  it("renders the component the HMR stage edits on the page it loads", () => {
    const files = fixtureFiles(presetNamed("small"), VERSIONS);
    const home = files.find((file) => file.path === "app/$page.js");
    const hot = files.find((file) => file.path === HOT_FILE);
    expect(home?.contents.includes('from "../components/HotCounter.js"')).toBe(true);
    expect(hot?.contents.includes(`"${HOT_MARKER}"`)).toBe(true);
  });

  it("names the presets there are when asked for one there is not", () => {
    expect(() => presetNamed("medium")).toThrow("small, large");
  });

  it("refuses to generate inside the repository", () => {
    const repository = path.join(os.tmpdir(), "uf-checkout");
    expect(() => refuseInsideRepository(path.join(repository, "bench"), repository)).toThrow(
      "inside this repository",
    );
    expect(() => refuseInsideRepository(repository, repository)).toThrow("inside this repository");
    // And a directory beside it is accepted, which a check that refused
    // everything would not do.
    refuseInsideRepository(path.join(os.tmpdir(), "uf-bench-toolchain"), repository);
  });
});

describe("the toolchain benchmark's summary of a stage", () => {
  it("takes the middle sample, or the mean of the middle two", () => {
    expect(summarise([5, 1, 3]).median).toBe(3);
    expect(summarise([4, 1, 3, 2]).median).toBe(2.5);
  });

  it("reports the spread beside the median", () => {
    const summary = summarise([10, 30, 20]);
    expect(summary.min).toBe(10);
    expect(summary.max).toBe(30);
    expect(summary.mean).toBe(20);
    expect(summarise([7, 7, 7]).stddev).toBe(0);
  });

  it("refuses to summarise a stage that timed nothing", () => {
    expect(() => summarise([])).toThrow("no samples");
  });

  it("prints durations the way they are read", () => {
    expect(formatDuration(8.44)).toBe("8.4 ms");
    expect(formatDuration(412.2)).toBe("412 ms");
    expect(formatDuration(3012.4)).toBe("3.01 s");
  });
});

/** How many `it(` cases the files hold. */
function cases(
  files: $ReadOnlyArray<{ readonly path: string, readonly contents: string, ... }>,
): number {
  return files
    .map((file) => (file.contents.match(/^\s+it\(/gm) ?? []).length)
    .reduce((sum, found) => sum + found, 0);
}

describe("the comparison tools' copies of the fixture", () => {
  it("are the same files every time they are generated", () => {
    const preset = presetNamed("small");
    const kinds: $ReadOnlyArray<RivalFixture> = ["vite", "next", "vitest", "bun"];
    for (const kind of kinds) {
      expect(rivalFiles(preset, kind)).toEqual(rivalFiles(preset, kind));
    }
  });

  it("carry exactly the tests uf's copy has, against each runner's own import", () => {
    for (const preset of PRESETS) {
      const tests = describeFixture(preset, fixtureFiles(preset, VERSIONS)).tests;
      const runners: $ReadOnlyArray<[RivalFixture, string]> = [
        ["vite", "vite-plus/test"],
        ["vitest", "vitest"],
        ["bun", "bun:test"],
      ];
      for (const [kind, runner] of runners) {
        const files = rivalFiles(preset, kind);
        expect(cases(files)).toBe(tests);
        const testFiles = files.filter((file) => file.path.endsWith(".test.ts"));
        expect(testFiles.length).toBe(preset.modules);
        expect(testFiles.every((file) => file.contents.includes(`from "${runner}";`))).toBe(true);
      }
    }
  });

  it("give Next.js a page for every route and no tests", () => {
    const preset = presetNamed("small");
    const paths = rivalFiles(preset, "next").map((file) => file.path);
    expect(paths.filter((file) => /^app\/r\d{3}\/page\.tsx$/.test(file)).length).toBe(
      preset.routes,
    );
    expect(paths.some((file) => file.includes(".test."))).toBe(false);
  });

  it("give official Flow the uf application in the spelling it accepts", () => {
    const files = flowFiles(fixtureFiles(presetNamed("small"), VERSIONS));
    const paths = files.map((file) => file.path);
    expect(paths.includes(".flowconfig")).toBe(true);
    expect(paths.includes("uf.config.js")).toBe(false);
    const sources = files.filter(
      (file) => file.path.endsWith(".js") && !file.path.startsWith("flow-typed/"),
    );
    expect(sources.some((file) => /\bmixed\b|\$ReadOnlyArray/.test(file.contents))).toBe(false);
    expect(files.find((file) => file.path === "app.js")?.contents).toContain("as unknown");
  });
});

describe("what the harness reads from other tools' output", () => {
  it("finds the client modules a page's inline Flight payload names", () => {
    const rows =
      '0:["$","main",null,{"children":"$L1"}]\n1:I["/components/HotCounter.js",["/components/HotCounter.js"],"HotCounter"]\n';
    const element = JSON.stringify(rows).replace(/</g, "\\u003c");
    expect(clientReferences(element)).toEqual([
      "/components/HotCounter.js",
      "/components/HotCounter.js",
    ]);
    expect(clientReferences(JSON.stringify({ bytes: "AAEC" }))).toEqual([]);
    expect(clientReferences("null")).toEqual([]);
  });

  it("reads CPU time from the second line of `times`, in bash's and dash's spelling", () => {
    expect(parseTimes("0m0.010s 0m0.020s\n0m1.250s 0m0.300s\n")).toBe(1550);
    expect(parseTimes("0m0.010000s 0m0.020000s\n1m0.500000s 0m0.250000s\n")).toBe(60750);
    expect(parseTimes("")).toBe(null);
  });

  it("reads a version out of whatever `--version` printed", () => {
    expect(versionIn("Version: 2.5.12")).toBe("2.5.12");
    expect(versionIn("vp v1.0.0-rc.0")).toBe("1.0.0-rc.0");
    expect(versionIn("Next.js v16.3.6")).toBe("16.3.6");
    expect(versionIn("nothing")).toBe("unknown");
  });

  it("reads the passing tests Vitest and Bun report", () => {
    expect(testsPassed(" Test Files  20 passed (20)\n      Tests  200 passed (200)")).toBe(200);
    expect(testsPassed(" 200 pass\n 0 fail\n 320 expect() calls")).toBe(200);
    expect(testsPassed("no tests found")).toBe(null);
  });
});

function row(
  id: string,
  median: number,
): { id: string, tool: string, stage: string, median: number } {
  const [tool, stage] = id.split("/");
  return { id, tool, stage, median };
}

describe("the regression gate", () => {
  const baseline = {
    results: [
      row("uf/build.cold/small", 1000),
      row("uf/hmr.message/small", 40),
      row("uf/install.cold/install", 5000),
      row("next/build.cold/small", 9000),
    ],
  };

  it("passes a run inside the line and fails one more than 20% and 25 ms over it", () => {
    const within = compare(baseline, [
      { results: [row("uf/build.cold/small", 1190), row("uf/hmr.message/small", 40)] },
    ]);
    expect(failures(within)).toEqual([]);
    const slower = compare(baseline, [
      { results: [row("uf/build.cold/small", 1300), row("uf/hmr.message/small", 40)] },
    ]);
    expect(failures(slower).map((verdict) => verdict.id)).toEqual(["uf/build.cold/small"]);
  });

  it("does not fail a short row on a few milliseconds, however large the percentage", () => {
    const verdicts = compare(baseline, [
      { results: [row("uf/build.cold/small", 1000), row("uf/hmr.message/small", 60)] },
    ]);
    expect(failures(verdicts)).toEqual([]);
  });

  it("compares the better of two runs, so one slow run of a stage is not a regression", () => {
    const verdicts = compare(baseline, [
      { results: [row("uf/build.cold/small", 1500), row("uf/hmr.message/small", 40)] },
      { results: [row("uf/build.cold/small", 1050)] },
    ]);
    expect(failures(verdicts)).toEqual([]);
  });

  it("fails a uf stage that stopped reporting, and gates neither installs nor other tools", () => {
    const verdicts = compare(baseline, [
      {
        results: [
          row("uf/hmr.message/small", 40),
          row("uf/install.cold/install", 20000),
          row("next/build.cold/small", 90000),
        ],
      },
    ]);
    expect(failures(verdicts).map((verdict) => `${verdict.id} ${verdict.outcome}`)).toEqual([
      "uf/build.cold/small missing",
    ]);
  });

  it("names a uf row the baseline does not have as new rather than failing on it", () => {
    const verdicts = compare(baseline, [
      {
        results: [
          row("uf/build.cold/small", 1000),
          row("uf/hmr.message/small", 40),
          row("uf/test.cold/suite", 300),
        ],
      },
    ]);
    expect(verdicts.find((verdict) => verdict.id === "uf/test.cold/suite")?.outcome).toBe("new");
    expect(failures(verdicts)).toEqual([]);
  });
});
