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
// reported with a `// expect: <text>` comment on the line above it, and this
// module reads both halves of the claim:
//
// * every marked line is reported, with a message containing that text — so a
//   change that makes one of them *stop* being an error fails here;
// * nothing else in the file is reported — so a change that makes a correct
//   use start failing fails here too, which is the half that says the fixture
//   is a set of narrow refusals rather than a file that is simply broken.
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
 * Two levels above the worker's project is this repository only while that
 * project is `tests/library`, and which project it is depends on how the
 * command was typed. `uf test#library` selects `tests/library` by name;
 * `uf test tests/library/ui.test.js` from the checkout selects the
 * *repository*, because a path is a filter over the project the command was
 * typed in and `UF_PROJECT_ROOT` is that project's root. Two levels above the
 * checkout holds no uf project at all, so `uf check` printed nothing, and what
 * a reader got was `SyntaxError: Unexpected end of JSON input` at the parse
 * below — a message about JSON for a mistake about a directory, in the
 * invocation someone reaches for when they want one file. That is
 * ubugeeei-prod/uf#313.
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
   * How many `// expect:` markers the fixture must have more than.
   *
   * Without it the test would pass on a fixture somebody had emptied, which is
   * the one way a negative type test fails silently: no markers means no
   * missing lines means green. It is per fixture rather than a constant
   * because each fixture's number is a claim about that fixture — losing four
   * of `field-paths.js`'s eleven cases is a regression that a shared floor of
   * four would not notice.
   */
  readonly atLeast: number,
};

/**
 * Hold one `tests/type-tests` fixture to its own `// expect:` markers.
 *
 * Runs `uf check tests/type-tests <alongside…> --json` from the repository
 * root and compares the diagnostics reported *against the fixture* with the
 * markers written in it. Throws through `expect`, so it is called from inside
 * an `it`.
 *
 * # Failure modes it reports rather than swallows
 *
 * A non-zero exit status is expected and ignored: a fixture is a file of
 * deliberate errors, so the checker is supposed to be unhappy. Three things
 * that would otherwise pass as success are not:
 *
 * * `uf check` printing nothing at all — the wrong directory, or a binary that
 *   did not start. `JSON.parse("")` says `Unexpected end of JSON input` and
 *   names neither the command, the directory, nor what was printed instead,
 *   which is the half of #313 that made a wrong directory take an afternoon to
 *   find rather than a minute. This says all three.
 * * a run that checked nothing — `status` and `filesChecked` are asserted, so
 *   a checker that skipped the files answers a failure rather than an empty
 *   diagnostic list that matches an empty set of expectations.
 * * a fixture with no markers left in it — see [`MisuseCheck.atLeast`].
 */
export function everyMisuseIsReported(check: MisuseCheck): void {
  const { fixture, alongside, atLeast } = check;
  const source = fs.readFileSync(path.join(repositoryRoot, fixture), "utf8").split("\n");
  const wanted = new Map<number, string>();
  source.forEach((line, index) => {
    const marker = line.match(/^\s*\/\/ expect: (.+)$/);
    if (marker != null) {
      // Lines are one-based, and the line that must fail is the next one.
      wanted.set(index + 2, marker[1]);
    }
  });
  expect(wanted.size).toBeGreaterThan(atLeast);

  const paths = ["tests/type-tests", ...alongside];
  const argv = ["check", ...paths, "--json"];
  const run = spawnSync(ufBinary, argv, {
    cwd: repositoryRoot,
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
  });
  if (run.stdout === "") {
    throw new Error(
      `\`uf ${argv.join(" ")}\` in ${repositoryRoot} printed nothing: ` +
        `status ${String(run.status)}, stderr ${JSON.stringify(run.stderr)}`,
    );
  }
  const report: CheckReport = JSON.parse(run.stdout);
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

  // And nothing else in the file is, which is what says the fixture describes
  // the types rather than merely being broken.
  const unexpected = [...reported.keys()]
    .filter((line) => !wanted.has(line))
    .map((line) => `${fixture}:${String(line)} ${String(reported.get(line))}`);
  expect(unexpected).toEqual([]);
}
