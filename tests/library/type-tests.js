// @flow
//
// The one harness every negative type test in this suite runs through.
//
// # What a negative type test is, and why it needs a harness at all
//
// Six packages here make a promise that no amount of rendering can hold them
// to: that a *particular misuse does not compile*. `Tabs.List` admits only
// tabs, a form's field path is checked against the shape of the values, a
// message's arguments are checked against the message. None of those is
// provable by calling anything — no assertion about behaviour can say that a
// different program would have been rejected — so each is proved the only way
// it can be: by running `uf check` over a file of deliberate errors and
// reading what it said.
//
// `tests/type-tests/` holds those files. Each marks a line that must be
// reported with a `// $FlowExpectedError[<code>] <what it is about>` comment on
// the line above it — Flow's own expected-error comment — and this module reads
// both halves of the claim:
//
// * every marked line is still an error with that code. Flow suppresses it, and
//   a suppression that no longer matches an error is reported by `uf check` as
//   "Unused suppression comment."; any such warning in a fixture fails here, so
//   a change that makes one of them *stop* being an error fails;
// * nothing else in the file is reported — so a change that makes a correct
//   use start failing fails here too, which is the half that says the fixture
//   is a set of narrow refusals rather than a file that is simply broken.
//
// Suppressed rather than left as errors so that `uf check` at the repository
// root — the command a contributor types, and the one CI gates — reports what
// is wrong with the code and not the refusals written here on purpose. What
// that costs is the message: the comment pins the error's *code*, and the words
// after it say what the error is about but are not compared with what the
// checker printed. Before this, each marker carried a fragment of the message
// the line had to produce, and the root check carried 291 errors that were not
// errors. ubugeeei-prod/uf#1451.
//
// # Why it is one module
//
// It was five copies of the same sixty lines, in `ui.test.js`,
// `assertion-types.test.js`, `form.test.js`, `state.test.js` and
// `server-actions.test.js`, differing only in which paths went to `uf check`.
// A sixth was wanted for `@uniflowed/i18n` — ubugeeei-prod/uf#568 — and a
// sixth copy is how a shared mechanism stops being one: the marker grammar,
// the "and only the misuses" half and the diagnostics that say which command
// printed nothing are all decisions that should be made once. This is that
// once.
//
// # Why the package goes to the checker with the fixture
//
// [`everyMisuseIsReported`] takes the packages the fixture imports and passes
// them to `uf check` in the *same* command, and that is load-bearing rather
// than tidy: `uf check` builds its module map out of the files it is asked
// about, so a relative import that leaves that set resolves to an any-typed
// value. After which every type in the fixture is `any`, every line of it
// passes, and the test proves the exact opposite of what it claims. The
// fixtures' own headers say why they are not simply written inside the
// packages they check.

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { expect } from "@uniflowed/test";

/**
 * This checkout, found by a directory it has rather than by counting `..`.
 *
 * Counting `..` was wrong even when there was one project this could run in.
 * `UF_PROJECT_ROOT` is the root of the project the command was typed in, and
 * two levels above it was this repository only while that project was
 * `tests/library` — two levels above the *checkout* holds no uf project at
 * all, so `uf check` printed nothing and what a reader got was
 * `SyntaxError: Unexpected end of JSON input` at the parse below: a message
 * about JSON for a mistake about a directory. That is ubugeeei-prod/uf#313.
 *
 * It is now wrong in a second way, which is why the fix is worth keeping
 * rather than simplifying away. The files that reach this harness are no
 * longer all the same distance from the root: `packages/ui/ui.test.js` and
 * `tests/library/server-actions.test.js` are two apiece, and nothing says the
 * next one will be.
 *
 * Searching upwards for something this repository has is true under both, and
 * is what `write-atomically.test.js` already does for the same reason.
 * `fileURLToPath(import.meta.url)` would say it more directly still, and is
 * itself one of the type errors `uf check` reports against this suite today —
 * see `story.test.js` — which is a poor thing for a module about type errors
 * to add another of.
 *
 * The marker is `tests/type-tests` rather than a file belonging to one
 * package, because this module serves every package: a harness that looked for
 * `packages/ui` would be the wrong error for a fixture about forms.
 */
export const repositoryRoot: string = (() => {
  const wanted = path.join("tests", "type-tests");
  const from = process.env.UF_PROJECT_ROOT ?? process.cwd();
  let directory = from;
  for (let up = 0; up < 8; up += 1) {
    if (fs.existsSync(path.join(directory, wanted))) return directory;
    directory = path.dirname(directory);
  }
  throw new Error(`could not find ${wanted} above ${from}`);
})();

/**
 * The binary running this suite, the way `lsp.test.js` names it.
 *
 * `uf test` puts its own path in `UF_BINARY`, so these tests check *this*
 * build rather than whatever `uf` happens to be on `PATH` — which on a
 * developer's machine is usually a release from last month.
 */
export const ufBinary: string = (() => {
  const binary = process.env.UF_BINARY;
  if (binary == null || binary === "") {
    throw new Error("UF_BINARY is not set: this test runs `uf check`, and `uf test` names it");
  }
  return binary;
})();

/**
 * One diagnostic of `uf check --json`, as this suite reads it.
 *
 * A message arrives as spans rather than as a string so that a renderer can
 * mark the code inside it, which is why every comparison here joins it back
 * together first. Exported because two tests run the checker over something
 * other than a fixture and read the same shape.
 */
export type CheckDiagnostic = {
  severity?: string,
  primary: { path: string, start: { line: number, column: number } },
  message: Array<{ kind: string, text: string }>,
};

/** The part of a `uf check --json` report this suite reads. */
export type CheckReport = {
  typeCheck: { status: string, filesChecked: number, diagnostics: Array<CheckDiagnostic> },
};

/** What a fixture is checked as. */
export type MisuseCheck = {
  /**
   * The fixture, relative to the repository root — usually
   * `path.join("tests", "type-tests", "<name>.js")`.
   */
  readonly fixture: string,
  /**
   * The packages the fixture imports, as `uf check` paths.
   *
   * They go to the checker in the same command as `tests/type-tests`; see the
   * module header for why that is not a detail.
   */
  readonly alongside: $ReadOnlyArray<string>,
  /**
   * How many `$FlowExpectedError` markers the fixture must have more than.
   *
   * Without it the test would pass on a fixture somebody had emptied, which is
   * the one way a negative type test fails silently: no markers means no
   * missing lines means green. It is per fixture rather than a constant
   * because each fixture's number is a claim about that fixture — losing four
   * of `field-paths.js`'s eleven cases is a regression that a shared floor of
   * four would not notice.
   */
  readonly atLeast: number,
  /**
   * Where the checker is run from; a fresh `uf check` per call when absent.
   *
   * See [`oneCheckPerCommand`] for when a file passes one.
   */
  readonly checker?: Checker,
};

/** What one `uf check` printed, before anything is read out of it. */
type CheckRun = {
  readonly status: number | null,
  readonly stdout: string,
  readonly stderr: string,
};

/** Runs `uf <argv>` from the repository root and hands back what it printed. */
export type Checker = (argv: $ReadOnlyArray<string>) => CheckRun;

/** A fresh `uf check` every time it is asked. */
const runEveryTime: Checker = (argv) => {
  const run = spawnSync(ufBinary, [...argv], {
    cwd: repositoryRoot,
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
  });
  // A checker that could not be started has no output at all rather than an
  // empty one; either way it is "printed nothing", which `checkReport` names.
  return {
    status: run.status,
    stdout: String(run.stdout ?? ""),
    stderr: String(run.stderr ?? run.error?.message ?? ""),
  };
};

/**
 * A checker that runs each distinct command once, for the file that made it.
 *
 * A fixture goes to `uf check` with the whole of `tests/type-tests` and the
 * packages it imports, so every fixture of one package is the same command:
 * `packages/ui/types.test.js` asked `uf check tests/type-tests packages/ui`
 * three times, and `assertion-types.test.js` asked its own command twice. The
 * command's answer does not depend on which fixture is about to be read out
 * of it, and each run cost as much as the last — about two seconds of CPU
 * warm, most of the file's time on a cold cache — so the second and third were
 * the same work done again. See ubugeeei-prod/uf#1430.
 *
 * Made by the test file, at its top level, rather than kept in this module.
 * A worker imports a test file afresh for every run of it but keeps this
 * module between files, so a cache here would outlive the file: a watch-mode
 * rerun after an edit to a fixture would read the answer about the fixture
 * before the edit. One per file, made where the file is, lives exactly as long
 * as the run it speeds up.
 *
 * What is kept is what the command printed, not a verdict, so every case still
 * reads it and fails on its own — a run that printed nothing fails each case
 * that asked for it, in the same words.
 *
 * `run` is what actually starts the checker, and is a parameter only so that
 * `./type-tests-harness.test.js` can count the starts without a binary.
 */
export function oneCheckPerCommand(run: Checker = runEveryTime): Checker {
  const runs = new Map<string, CheckRun>();
  return (argv) => {
    const key = JSON.stringify(argv);
    const known = runs.get(key);
    if (known != null) return known;
    const ran = run(argv);
    runs.set(key, ran);
    return ran;
  };
}

/**
 * What `uf check <paths> --json` reported, and proof that it checked something.
 *
 * Three things that would otherwise pass as success are refused here rather
 * than in each caller: printing nothing at all (the wrong directory, or a
 * binary that did not start — `JSON.parse("")` names neither, which is the
 * half of #313 that made a wrong directory take an afternoon to find), and a
 * report whose type check did not run or checked no file, which would match an
 * empty set of expectations.
 */
function checkReport(paths: $ReadOnlyArray<string>, checker: Checker): CheckReport {
  // `--no-lint`: only `typeCheck` is read here, and on a warm cache the lint
  // was nine tenths of what each of these commands cost.
  const argv = ["check", ...paths, "--json", "--no-lint"];
  const run = checker(argv);
  if (run.stdout === "") {
    throw new Error(
      `\`uf ${argv.join(" ")}\` in ${repositoryRoot} printed nothing: ` +
        `status ${String(run.status)}, stderr ${JSON.stringify(run.stderr)}`,
    );
  }
  const report: CheckReport = JSON.parse(run.stdout);
  expect(report.typeCheck.status).toBe("checked");
  expect(report.typeCheck.filesChecked).toBeGreaterThan(0);
  return report;
}

export type CleanCheck = {
  readonly fixture: string,
  readonly alongside: $ReadOnlyArray<string>,
  /** As [`MisuseCheck.checker`]. */
  readonly checker?: Checker,
};

/**
 * Hold one `tests/type-tests` fixture to its own `$FlowExpectedError` markers.
 *
 * Runs `uf check tests/type-tests <alongside…> --json` from the repository
 * root and reads what it reported *against the fixture*: nothing at all is the
 * only passing answer. Every marked line is an error Flow suppressed, so an
 * error there is an unmarked line reporting; and a marker that no longer
 * matches an error is reported as "Unused suppression comment.", which is the
 * misuse the fixture exists to refuse starting to compile. Throws through
 * `expect`, so it is called from inside an `it`.
 *
 * # Failure modes it reports rather than swallows
 *
 * * `uf check` printing nothing at all — the wrong directory, or a binary that
 *   did not start. `JSON.parse("")` says `Unexpected end of JSON input` and
 *   names neither the command, the directory, nor what was printed instead,
 *   which is the half of #313 that made a wrong directory take an afternoon to
 *   find rather than a minute. This says all three.
 * * a run that checked nothing — `status` and `filesChecked` are asserted, so
 *   a checker that skipped the files answers a failure rather than an empty
 *   diagnostic list, which would otherwise be the passing answer.
 * * a fixture with no markers left in it — see [`MisuseCheck.atLeast`].
 */
export function everyMisuseIsReported(check: MisuseCheck): void {
  const { fixture, alongside, atLeast, checker = runEveryTime } = check;
  const source = fs.readFileSync(path.join(repositoryRoot, fixture), "utf8").split("\n");
  const markers = new Map<number, string>();
  source.forEach((line, index) => {
    const marker = line.match(/^\s*\/\/ \$FlowExpectedError\[([^\]]+)\]/);
    if (marker != null) {
      // One-based, and it is the marker's own line: that is where Flow reports
      // a suppression that suppressed nothing.
      markers.set(index + 1, marker[1]);
    }
  });
  expect(markers.size).toBeGreaterThan(atLeast);

  const report = checkReport(["tests/type-tests", ...alongside], checker);

  const stopped = [];
  const unexpected = [];
  for (const diagnostic of report.typeCheck.diagnostics) {
    if (!diagnostic.primary.path.endsWith(fixture)) continue;
    const line = diagnostic.primary.start.line;
    const said = diagnostic.message.map((span) => span.text).join("");
    const code = markers.get(line);
    if (code != null && said.startsWith("Unused suppression comment")) {
      stopped.push(`${fixture}:${String(line + 1)} is no longer reported as ${code}`);
    } else {
      unexpected.push(`${fixture}:${String(line)} ${said}`);
    }
  }
  // Every marked line is still an error with the code its marker names.
  expect(stopped).toEqual([]);
  // And nothing else in the file is, which is what says the fixture describes
  // the types rather than merely being broken.
  expect(unexpected).toEqual([]);
}

/** Hold a fixture that must type-check cleanly to that promise. */
export function noDiagnosticsAreReported(check: CleanCheck): void {
  const { fixture, alongside, checker = runEveryTime } = check;
  const report = checkReport(["tests/type-tests", ...alongside], checker);

  const unexpected = [];
  for (const diagnostic of report.typeCheck.diagnostics) {
    if (diagnostic.primary.path.endsWith(fixture)) {
      unexpected.push(
        `${fixture}:${String(diagnostic.primary.start.line)} ${diagnostic.message
          .map((span) => span.text)
          .join("")}`,
      );
    }
  }
  expect(unexpected).toEqual([]);
}
