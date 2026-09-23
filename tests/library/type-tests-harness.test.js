// @flow
//
// The harness in `./type-tests.js`, on its own: what it promises about how
// often it starts `uf check`, which no fixture-reading test can see.

import { describe, expect, it } from "@uniflowed/test";

import type { Checker } from "./type-tests.js";
import { oneCheckPerCommand } from "./type-tests.js";

/** A stand-in for `uf`, counting the commands it is asked to run. */
function counting(): { run: Checker, started: Array<string> } {
  const started: Array<string> = [];
  const run: Checker = (argv) => {
    started.push(argv.join(" "));
    return { status: 1, stdout: `{"n":${String(started.length)}}`, stderr: "" };
  };
  return { run, started };
}

describe("one check per command", () => {
  it("starts a command once, however many cases ask for it", () => {
    const { run, started } = counting();
    const checker = oneCheckPerCommand(run);

    const first = checker(["check", "tests/type-tests", "packages/ui", "--json"]);
    const second = checker(["check", "tests/type-tests", "packages/ui", "--json"]);

    expect(started).toEqual(["check tests/type-tests packages/ui --json"]);
    // The same answer, not a second one that happens to look alike.
    expect(second).toBe(first);
  });

  it("starts each different command, and keeps their answers apart", () => {
    const { run, started } = counting();
    const checker = oneCheckPerCommand(run);

    const ui = checker(["check", "tests/type-tests", "packages/ui", "--json"]);
    const form = checker(["check", "tests/type-tests", "packages/form", "--json"]);
    // Paths that join to the same string are still different commands.
    checker(["check", "tests/type-tests packages/ui", "--json"]);

    expect(started).toEqual([
      "check tests/type-tests packages/ui --json",
      "check tests/type-tests packages/form --json",
      "check tests/type-tests packages/ui --json",
    ]);
    expect(ui.stdout).toBe('{"n":1}');
    expect(form.stdout).toBe('{"n":2}');
  });

  it("keeps nothing between two checkers, as two files would each make one", () => {
    const { run, started } = counting();

    oneCheckPerCommand(run)(["check", "packages/ui", "--json"]);
    oneCheckPerCommand(run)(["check", "packages/ui", "--json"]);

    expect(started).toEqual(["check packages/ui --json", "check packages/ui --json"]);
  });
});
