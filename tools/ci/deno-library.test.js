// @flow
import { expect, it } from "@uniflowed/test";
import { classify } from "./deno-library.js";

it("fails native-addon errors in Vite and unrelated files", () => {
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
  expect(report.failures.length).toBe(2);
  expect(report.failures[0].file).toBe("packages/vite/flight.test.js");
  expect(report.failures[1].file).toBe("packages/ui/ui.test.js");
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
it("fails when Deno exits during payload hydration", () => {
  const report = classify({
    passed: 0,
    skipped: 0,
    tests: [
      {
        file: "tests/library/payload.test.js",
        name: "the browser applying a payload > hydrates from the rows the document carried, without running the loader again",
        status: "passed",
        failures: [],
      },
    ],
    fileReports: [
      {
        file: "tests/library/payload.test.js",
        status: "host-failed",
        reason:
          "the host failed: uncaught exception: The server could not finish this Suspense boundary, likely due to an error during server rendering. Switched to client rendering.",
      },
    ],
  });
  expect(report.failures.length).toBe(1);
});
it("does not accept another uncaught exception in the payload tests", () => {
  const report = classify({
    passed: 0,
    skipped: 0,
    tests: [
      {
        file: "tests/library/payload.test.js",
        name: "the browser applying a payload > hydrates from the rows the document carried, without running the loader again",
        status: "passed",
        failures: [],
      },
    ],
    fileReports: [
      {
        file: "tests/library/payload.test.js",
        status: "host-failed",
        reason: "the host failed: uncaught exception: something else",
      },
    ],
  });
  expect(report.failures.length).toBe(1);
});

it("fails the former Relay and document storage exceptions", () => {
  const report = classify({
    passed: 0,
    skipped: 0,
    fileReports: [],
    tests: [
      {
        file: "packages/vite/relay.test.js",
        name: "Relay transform",
        status: "failed",
        failures: [{ message: "globalsBuiltinLower is not iterable" }],
      },
      {
        file: "packages/react-testing/dom-storage.test.js",
        name: "document storage probe",
        status: "failed",
        failures: [{ message: "Loading unprepared module: /packages/react/react" }],
      },
    ],
  });
  expect(report.failures.length).toBe(2);
  expect(report.failures[0].file).toBe("packages/vite/relay.test.js");
  expect(report.failures[1].file).toBe("packages/react-testing/dom-storage.test.js");
});
