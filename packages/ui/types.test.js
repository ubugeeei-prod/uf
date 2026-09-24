// @flow
//
// What `uf check` says about `@uniflowed/ui`.
//
// Two of this package's promises are types rather than behaviour, and no
// amount of rendering can hold it to either: that no part turns React's `key`
// into a `mixed`, and that a misused `side`, `align`, `role` or child is an
// error at the call site rather than an overlay in the wrong place at run
// time. Both are held here, by running the checker and reading what it said.
// `../../tests/library/type-tests.js` holds what it takes to run it — the
// checkout, the binary `uf test` named, and the marker harness the fixture
// blocks share with five other suites.
//
// # Why this is not in `ui.test.js`
//
// It was, and the clock is the reason it moved. Every block here starts a
// `uf check` process, which is about three quarters of a second each; the
// five hundred rendering cases beside them cost about a second in total. A
// file runs in one worker from end to end, so those four seconds were four
// seconds of the suite's whole wall clock, and `ui.test.js` was the longest
// file in it by a factor of two. Split, the checks run beside the rendering
// rather than after it.
//
// The name follows `tests/library/assertion-types.test.js` and
// `simple-sns-types.test.js`, which are the same kind of file for the same
// reason.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

import { describe, expect, it } from "@uniflowed/test";

import type { CheckDiagnostic } from "../../tests/library/type-tests.js";
import {
  everyMisuseIsReported,
  oneCheckPerCommand,
  repositoryRoot as repository,
  ufBinary as UF,
} from "../../tests/library/type-tests.js";

// The three fixture blocks below are one command — `uf check tests/type-tests
// packages/ui` — read three ways, so the file runs it once and each block
// reads its own fixture out of the answer. `oneCheckPerCommand` says why it is
// made here rather than shared between files.
const checker = oneCheckPerCommand();

/** What `uf check packages/ui --json` says, as the first block below reads it. */
type PackageReport = {
  typeCheck: {
    status: string,
    filesChecked: number,
    /** How many files the command named, before their imports were added. */
    requested: number,
    diagnostics: Array<CheckDiagnostic>,
  },
};

describe("the props a part spreads onto its element", () => {
  // A type is a promise the same way a role is, and this is the only test here
  // that can hold one to it.
  //
  // Every part takes `...rest: Rest` and spreads it onto an intrinsic. `Rest`
  // — `packages/ui/internal/merge-props.js` — names `key` out of its indexer,
  // because React's `key` is `string | number` and an indexer answers `mixed`
  // for every name. Widen it back to a bare `{ readonly [string]: mixed }` and
  // `uf check` reports "Cannot create button element because in property key"
  // once for every element the package renders: thirty-two of them, which is
  // what #206 was.
  //
  // Scoped to that one family on purpose. `packages/ui` still reports
  // `value-as-type` errors for `React.Node` and `React.Context`, because
  // nothing resolves a module for `@uniflowed/react` and the import is typed
  // `any` — a different bug, with a different fix, and not one this test
  // should start failing over.
  //
  // And scoped to what the package *ships*. `uf check packages/ui` now reads
  // this file too, because this file is in `packages/ui` — that is what
  // co-locating the suite means. The claim above is about the elements the
  // package renders, so a diagnostic against a test file is not evidence for
  // or against it, and counting one would make the suite's own call sites the
  // subject. There is one today: the `Plans` component below spreads
  // `Field.Control`'s render props onto a `RadioGroup.Root`, and the checker
  // has an opinion about the `key` in them that nothing had asked for until
  // this file moved. That is a real finding about the API and it belongs in
  // an issue about `Field.Control`, not in an assertion about `merge-props.js`.

  it("does not make React's key mixed", () => {
    const run = spawnSync(UF, ["check", "packages/ui", "--json", "--no-lint"], {
      cwd: repository,
      encoding: "utf8",
      maxBuffer: 32 * 1024 * 1024,
    });
    // A non-zero status is expected: the package still has the `value-as-type`
    // errors above. The answer is on stdout either way — and when it is not,
    // this says so. `JSON.parse("")` reports `Unexpected end of JSON input`
    // and names neither the command, the directory, nor what the command said
    // instead, which is the half of #313 that made a wrong directory take an
    // afternoon to find rather than a minute.
    if (run.stdout === "") {
      throw new Error(
        `\`uf check packages/ui --json\` in ${repository} printed nothing: ` +
          `status ${String(run.status)}, stderr ${JSON.stringify(run.stderr)}`,
      );
    }
    const report: PackageReport = JSON.parse(run.stdout);
    // Without this the test would pass just as happily on a run that checked
    // nothing at all.
    expect(report.typeCheck.status).toBe("checked");
    expect(report.typeCheck.filesChecked).toBeGreaterThan(0);

    const keyed = report.typeCheck.diagnostics
      .filter((diagnostic) => !diagnostic.primary.path.endsWith(".test.js"))
      .map((diagnostic) => ({
        at: `${diagnostic.primary.path}:${String(diagnostic.primary.start.line)}`,
        said: diagnostic.message.map((span) => span.text).join(""),
      }))
      .filter((diagnostic) => diagnostic.said.includes("in property key"));
    expect(keyed).toEqual([]);
    // And the checker read the package rather than only its tests, which is
    // the way the filter above could have emptied the list it is asserting on.
    // Counted from what it was asked to check rather than from what it
    // reported: this used to require a diagnostic outside the tests, and so
    // failed the day the package's sources checked clean.
    const tests = fs
      .readdirSync(path.join(repository, "packages", "ui"))
      .filter((name) => name.endsWith(".test.js"));
    expect(report.typeCheck.requested).toBeGreaterThan(tests.length);
  });
});

describe("a side and an alignment are unions, not strings", () => {
  // The other promise a type makes, and the other one no amount of rendering
  // can check. `internal/anchor.js` says a side is one of four names and an
  // alignment one of three; the claim that follows is that a consumer who
  // misspells one is stopped by the checker rather than by a reader finding an
  // overlay in the wrong place.
  //
  // `tests/type-tests/anchoring.js` is the misuse, written down.

  it("reports every misuse, and only the misuses", () => {
    everyMisuseIsReported({
      fixture: path.join("tests", "type-tests", "anchoring.js"),
      alongside: ["packages/ui"],
      atLeast: 4,
      checker,
    });
  });
});

describe("a wrong child is a type error and not a review comment", () => {
  // The strongest claim `packages/ui/index.js` makes, and the one nothing here
  // held: `Tabs.List` declares `renders* Tabs.Tab`, so a `<button>` in a tab
  // list does not compile. Thirteen containers in this package state a constraint
  // like that — `Menu.Body`, `Combobox.List`, `Select.List`, `Toast.Region`,
  // `Pagination.Content` and the rest — and every one of them was an unverified
  // promise: they were checked by hand against a scratch file while `select.js`
  // and `toast.js` were written, and a scratch file survives no refactor. That
  // is ubugeeei-prod/uf#358.
  //
  // It is checked here rather than by rendering anything because there is
  // nothing to render. The failure a `renders*` prevents does not reach a
  // browser: it is a `<button>` announced as "button" where the reader expected
  // "tab, 2 of 5", in a build that never happened.
  //
  // Both directions are in the fixture, which is the part worth keeping. A
  // constraint that stopped rejecting a `<div>` would take the guarantee away
  // and nothing else would notice; one that started rejecting the parts it
  // exists to admit would take the library away, and this test would name which
  // container did it.
  //
  // `tests/type-tests/composition.js` is the misuse, written down.

  it("reports every misuse, and only the misuses", () => {
    everyMisuseIsReported({
      fixture: path.join("tests", "type-tests", "composition.js"),
      alongside: ["packages/ui"],
      atLeast: 4,
      checker,
    });
  });
});

describe("an edge, a role, an alphabet and an orientation are unions too", () => {
  // The same claim, for the dialog-shaped components, the three that replace
  // something the browser already does, and the rule between two of them. A
  // sheet's `side`, a sidebar's — which has two members rather than four,
  // because a sidebar is never along the top — a modal's `role`, what a
  // one-time code is made of, and which way a `Separator` runs: five unions
  // whose misuse has no symptom at run time and none in a screenshot.
  //
  // `tests/type-tests/overlays.js` is the misuse, written down.

  it("reports every misuse, and only the misuses", () => {
    everyMisuseIsReported({
      fixture: path.join("tests", "type-tests", "overlays.js"),
      alongside: ["packages/ui"],
      atLeast: 4,
      checker,
    });
  });
});
