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
// the benchmark would generate into and then delete.

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
import { summarise } from "./measure.js";
import { formatDuration } from "./report.js";

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
