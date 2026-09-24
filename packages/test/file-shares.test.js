// @flow
//
// One file run as shares: `RunOptions.part`.
//
// `uf test` splits a file long enough to hold up a run across several workers
// when a project sets `test.splitFiles`. Each worker imports the whole file and
// is told which share of its cases is its own, and the promise the shares make
// between them is the one a whole run makes on its own: every case reported
// exactly once, in the order the file declares them, with only the setup a
// share's own cases need.

import { describe, expect, it } from "@uniflowed/test";
import {
  beforeAll as registerBeforeAll,
  describe as registerSuite,
  it as registerCase,
  reset,
} from "./internal/registry.js";
import { type Result, run as runRegistered } from "./internal/run.js";

/** Register a file of seven cases, two of them in a nested suite. */
function registerSevenCases(ran: Array<string>, setUp: Array<string>): void {
  registerCase("one", () => {
    ran.push("one");
  });
  registerSuite("outer", () => {
    registerBeforeAll(() => {
      setUp.push("outer");
    });
    registerCase("two", () => {
      ran.push("two");
    });
    registerCase.skip("three", () => {
      ran.push("three");
    });
    registerSuite("inner", () => {
      registerBeforeAll(() => {
        setUp.push("inner");
      });
      registerCase("four", () => {
        ran.push("four");
      });
      registerCase.todo("five");
    });
  });
  registerCase("six", () => {
    ran.push("six");
  });
  registerCase("seven", () => {
    ran.push("seven");
  });
}

/** Run share `index` of `count`, and hand back what it reported and did. */
async function runShare(
  index: number,
  count: number,
): Promise<{| names: Array<string>, ran: Array<string>, setUp: Array<string> |}> {
  const ran: Array<string> = [];
  const setUp: Array<string> = [];
  const results: Array<Result> = [];
  reset();
  registerSevenCases(ran, setUp);
  await runRegistered({ file: "virtual.test.js", part: { index, count } }, (result) => {
    results.push(result);
  });
  return { names: results.map((result) => result.name), ran, setUp };
}

describe("a file run as shares", () => {
  it("reports every case exactly once between them, in the order the file declares them", async () => {
    const shares = [await runShare(0, 3), await runShare(1, 3), await runShare(2, 3)];
    expect(shares.flatMap((share) => share.names)).toEqual([
      "one",
      "outer > two",
      "outer > three",
      "outer > inner > four",
      "outer > inner > five",
      "six",
      "seven",
    ]);
    // Contiguous, so each share is a run of the file rather than a scatter.
    expect(shares.map((share) => share.names.length)).toEqual([2, 2, 3]);
  });

  it("runs only its own cases, and sets up only for them", async () => {
    const first = await runShare(0, 3);
    expect(first.ran).toEqual(["one", "two"]);
    expect(first.setUp).toEqual(["outer"]);

    const second = await runShare(1, 3);
    // `three` is a skip and `five` a todo: reported, never run.
    expect(second.ran).toEqual(["four"]);
    expect(second.setUp).toEqual(["outer", "inner"]);

    const third = await runShare(2, 3);
    expect(third.ran).toEqual(["six", "seven"]);
    expect(third.setUp).toEqual([]);
  });

  it("is the whole file when there is one share", async () => {
    const whole = await runShare(0, 1);
    expect(whole.names.length).toBe(7);
    expect(whole.ran).toEqual(["one", "two", "four", "six", "seven"]);
  });

  it("leaves a share with no case of its own empty rather than failing it", async () => {
    // More shares than cases is the runner's to avoid; if it happens anyway
    // the cases are still each reported once.
    const shares = [];
    for (let index = 0; index < 9; index += 1) {
      shares.push(await runShare(index, 9));
    }
    expect(shares.flatMap((share) => share.names).length).toBe(7);
    expect(shares.filter((share) => share.names.length === 0).length).toBe(2);
  });
});
