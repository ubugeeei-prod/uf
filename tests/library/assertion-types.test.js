// @flow
//
// What `uf check` says about the two surfaces that used to be `any`.
//
// `@uniflowed/test`'s `expect` and `@uniflowed/react-testing`'s `fireEvent` are
// the same problem twice: a value that answers to a name decided while the
// program runs, published from packages whose entire purpose is testing *typed*
// code, and typed as `any` because nothing else could be assigned to what they
// were. So `expect(user).toBaa(1)` and `fireEvent.clcik(button)` were calls
// nobody's checker refused — and neither fails at run time either, because a
// matcher that does not exist asserts nothing and an event nobody listens for
// changes nothing. They pass, and they read in review as tests.
//
// Both are written-out listings now, and the claim a listing makes is not
// provable by running anything: no assertion about behaviour can say that a
// *different* program would have been rejected. So it is proved the only way it
// can be — by running the checker over code that must fail and reading what it
// said.
//
// `tests/type-tests/matchers.js` and `tests/type-tests/event-names.js` are
// those misuses, written down. Each marks the lines that must be reported with
// a `// expect:` comment, and this reads both halves: a marked line that stops
// being an error fails here, and so does an unmarked line that starts being
// one.

import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "@uniflowed/test";
import { spawnSync } from "node:child_process";

// This checkout, found by a file it has rather than by counting `..`, for the
// reason `ui.test.js` sets out at length: which project `uf test` selected
// depends on how the command was typed, and two levels above the worker's
// project is this repository only under one of them.
const repository: string = (() => {
  const wanted = path.join("packages", "test", "internal", "expect.js");
  const from = process.env.UF_PROJECT_ROOT ?? process.cwd();
  let directory = from;
  for (let up = 0; up < 8; up += 1) {
    if (fs.existsSync(path.join(directory, wanted))) return directory;
    directory = path.dirname(directory);
  }
  throw new Error(`could not find ${wanted} above ${from}`);
})();

// The binary running this suite: `uf test` puts its own path in `UF_BINARY`, so
// this checks *this* build rather than whatever `uf` is on PATH.
const UF: string = (() => {
  const binary = process.env.UF_BINARY;
  if (binary == null || binary === "") {
    throw new Error("UF_BINARY is not set: this test runs `uf check`, and `uf test` names it");
  }
  return binary;
})();

// The part of `uf check --json` this reads. A message arrives as spans rather
// than a string so that a renderer can mark the code inside it, which is why
// the comparison below joins it back together first.
type Diagnostic = {
  primary: { path: string, start: { line: number, column: number } },
  message: Array<{ kind: string, text: string }>,
};
type Report = {
  typeCheck: { status: string, filesChecked: number, diagnostics: Array<Diagnostic> },
};

/**
 * Hold one `tests/type-tests` fixture to its own `// expect:` markers.
 *
 * Both packages go to the checker along with the fixtures, in one command, and
 * that is load-bearing: `uf check` builds its module map from the files it is
 * asked about, so a relative import that leaves that set resolves to an
 * any-typed value — after which `expect` and `fireEvent` are `any` again, every
 * line of both fixtures passes, and this test proves the opposite of what it
 * says. The fixtures' own headers say why they are not inside the packages.
 */
function everyMisuseIsReported(fixture: string, atLeast: number): void {
  const source = fs.readFileSync(path.join(repository, fixture), "utf8").split("\n");
  const wanted = new Map<number, string>();
  source.forEach((line, index) => {
    const marker = line.match(/^\s*\/\/ expect: (.+)$/);
    if (marker != null) {
      // Lines are one-based, and the line that must fail is the next one.
      wanted.set(index + 2, marker[1]);
    }
  });
  // Without this the test would pass on a fixture somebody had emptied.
  expect(wanted.size).toBeGreaterThan(atLeast);

  const run = spawnSync(
    UF,
    ["check", "tests/type-tests", "packages/test", "packages/react-testing", "--json"],
    { cwd: repository, encoding: "utf8", maxBuffer: 32 * 1024 * 1024 },
  );
  if (run.stdout === "") {
    throw new Error(
      "`uf check tests/type-tests packages/test packages/react-testing --json` in " +
        `${repository} printed nothing: status ${String(run.status)}, ` +
        `stderr ${JSON.stringify(run.stderr)}`,
    );
  }
  const report: Report = JSON.parse(run.stdout);
  // Without this the test would pass just as happily on a run that checked
  // nothing at all.
  expect(report.typeCheck.status).toBe("checked");
  expect(report.typeCheck.filesChecked).toBeGreaterThan(0);

  const reported = new Map<number, string>();
  for (const diagnostic of report.typeCheck.diagnostics) {
    if (diagnostic.primary.path.endsWith(fixture)) {
      reported.set(
        diagnostic.primary.start.line,
        diagnostic.message.map((span) => span.text).join(""),
      );
    }
  }

  const missing = [];
  for (const [line, expected] of wanted) {
    const said = reported.get(line);
    if (said == null || !said.includes(expected)) {
      missing.push(`${fixture}:${String(line)} should say "${expected}", said ${String(said)}`);
    }
  }
  // Every marked line is an error, with the message the fixture predicted.
  expect(missing).toEqual([]);

  // And nothing else in the file is: every correct assertion below the marked
  // ones still checks, which is what says the listing describes the value
  // rather than merely refusing things.
  const unexpected = [...reported.keys()]
    .filter((line) => !wanted.has(line))
    .map((line) => `${fixture}:${String(line)} ${String(reported.get(line))}`);
  expect(unexpected).toEqual([]);
}

describe("a matcher is a name the checker knows", () => {
  // `expect` was `$FlowFixMe`, and so was everything it handed back, so a
  // misspelt matcher and an argument of the wrong type were both silent. Every
  // test in this repository is written against it.
  //
  // `tests/type-tests/matchers.js` is the misuse, written down. Its tail is
  // the other half of the claim: four correct assertions that a `toBe` carrying
  // the received value's type would refuse, which is why it does not carry one.

  it("reports every misuse, and only the misuses", () => {
    everyMisuseIsReported(path.join("tests", "type-tests", "matchers.js"), 6);
  });
});

describe("an event name is a name the checker knows", () => {
  // The same claim for `fireEvent`, which was a `Proxy` and so could carry no
  // type at all. Two of the fixture's markers are the price of the table
  // rather than a bug in it — an unlisted name and the DOM's lower-case
  // spelling both stop working — and they are marked so that the loss stays a
  // decision somebody made.
  //
  // `tests/type-tests/event-names.js` is the misuse, written down.

  it("reports every misuse, and only the misuses", () => {
    everyMisuseIsReported(path.join("tests", "type-tests", "event-names.js"), 2);
  });
});
