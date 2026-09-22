// @flow
import { expect, it } from "@uniflowed/test";
import { classify } from "./deno-library.js";

it("reports a named native-addon exception without accepting the same error in another file", () => {
  const reason = "failed to load: Error: Cannot find native binding.";
  const report = classify({
    passed: 1,
    skipped: 0,
    tests: [],
    fileReports: [
      { file: "packages/vite/flight.test.js", status: "load-failed", reason },
      { file: "packages/ui/ui.test.js", status: "load-failed", reason },
    ],
  });
  expect(report.runtimeSkips.length).toBe(1);
  expect(report.failures.length).toBe(1);
  expect(report.failures[0].file).toBe("packages/ui/ui.test.js");
});
it("fails a different error in an otherwise exempted file", () => {
  const report = classify({
    passed: 0,
    skipped: 0,
    tests: [],
    fileReports: [
      {
        file: "packages/vite/flight.test.js",
        status: "load-failed",
        reason: "unexpected syntax error",
      },
    ],
  });
  expect(report.runtimeSkips.length).toBe(0);
  expect(report.failures.length).toBe(1);
});
it("does not accept an unexplained worker exit", () => {
  const report = classify({
    passed: 0,
    skipped: 0,
    tests: [],
    fileReports: [
      {
        file: "tests/library/payload.test.js",
        status: "host-failed",
        reason: "the host failed: the worker exited (exit status: 1)",
      },
    ],
  });
  expect(report.failures.length).toBe(1);
});
