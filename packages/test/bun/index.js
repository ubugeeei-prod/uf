// @noflow
//
// Plain JavaScript: `bun:test` has no Flow library definition, and the only
// thing that ever imports this is a `bun test` that `uf test` started.
//
// `@uniflowed/test`, when the suite is run by `bun test`.
//
// A project that writes `test: { runner: "bun" }` in `uf.config.js` has its
// suite run by `bun test` instead of uf's own runner, and its test files do not
// change: they still import `@uniflowed/test`. `uf test` starts Bun with the
// `uniflowed-bun-test` export condition, and this package's `exports` sends
// `@uniflowed/test` here under that condition and to `../index.js` under every
// other. uf's own runner never sets it, on Node or on Bun, so the only process
// that meets this file is one `uf test` started to hand a suite to Bun.
//
// # Why a condition rather than a plugin
//
// A `Bun.plugin` `onResolve` is not consulted for a bare specifier that
// resolves to an installed package, and `@uniflowed/test` is installed in every
// project that uses it. Tried on Bun 1.3.13, the test file imported uf's real
// API, registered its cases where Bun could not see them, and `bun test`
// printed "Ran 0 tests across 1 file" and exited 0 — a green run over a suite
// that ran nothing. Resolution is the one place the choice is made before a
// test file's imports run. See ubugeeei-prod/uf#942.
//
// # What maps, and what refuses
//
// The registration API, the hooks and `expect` are Bun's own, with the
// modifiers both runners give them: `.only`, `.skip`, `.todo` and `.each`
// (whose `%s %j %d %i` placeholders both runners format). `fn`, `spyOn`, and
// the `uft` members Bun has an equivalent for — the fake clock and the mock
// resets — are Bun's. uf's host-agnostic helpers are uf's: `uft.waitFor` and
// `uft.waitUntil` poll with `setTimeout` under either runner, `uft.mocked` is
// the identity function, and `equals` and `render` are pure.
//
// Everything else raises `UnsupportedError` at the moment it is used, naming
// what was called and why there is nothing here to give it — never an
// approximation that would report a different result:
//
// * `it.skipBecause`: Bun's skip carries no reason into its report, and a skip
//   whose reason quietly disappears is what `skipBecause` exists to prevent.
// * uf's DOM matchers and `toHaveNoAxeViolations`: Bun's `expect` has none.
// * `uft.stubEnv`, `uft.stubGlobal` and their `unstubAll` pairs: uf puts a stub
//   back before the next file, and `bun test` runs every file in one process
//   where nothing would.
// * Module mocking: Bun's `mock.module` rewrites modules that have already
//   imported the real one, which is a different promise from `uft.mock`'s.
// * `uft.advanceTimersByTimeAsync`: Bun's fake clock has no asynchronous
//   advance, and flushing microtasks between timers is the whole of what it
//   promises.
// * `AssertionError` and `RunawayTimersError`: Bun throws errors of its own, so
//   a test comparing a failure against uf's class would be comparing it against
//   a class nothing throws.

import * as bun from "bun:test";

import { equals, render } from "../internal/equality.js";
import { mocked, waitFor, waitUntil } from "../internal/namespace.js";
import { DEFAULT_TIMEOUT_MS, NAME_SEPARATOR } from "../internal/run.js";
import { UnsupportedError } from "../internal/unsupported.js";

export { UnsupportedError, equals, render, DEFAULT_TIMEOUT_MS, NAME_SEPARATOR };
export { afterAll, afterEach, beforeAll, beforeEach } from "bun:test";

/** What every refusal here says about where the suite is running. */
const UNDER_BUN = "the suite is being run by `bun test` (`test.runner` in uf.config.js)";

/**
 * The error for something that is not a `uft` member.
 *
 * Still an `UnsupportedError`, because that is the class a test catches;
 * only the message changes, since `UnsupportedError`'s is written for `uft`
 * members and `it.skipBecause` or a matcher is not one.
 */
class RunnerUnsupportedError extends UnsupportedError {
  constructor(binding, reason) {
    super(binding, reason);
    this.message = `${binding} is not available: ${reason}`;
  }
}

/** A function that refuses, for a binding with no equivalent under Bun. */
function refusing(binding, reason, Refusal = RunnerUnsupportedError) {
  return () => {
    throw new Refusal(binding, `${UNDER_BUN}, and ${reason}`);
  };
}

/** A class that refuses to be constructed or compared against. */
function refusingClass(name, reason) {
  const refuse = refusing(name, reason);
  return class {
    constructor() {
      refuse();
    }

    static [Symbol.hasInstance]() {
      return refuse();
    }
  };
}

/** The placeholders uf's `.each` substitutes a row into. */
const ROW_TOKEN = /%[sjdi]/g;

/**
 * A row's case name, written the way uf's own runner writes it.
 *
 * Bun's `.each` differs from uf's twice over: it spreads an array row into the
 * body's arguments where uf hands the body the row whole, and its JUnit report
 * records the name with the placeholder still in it (`formats row %s`, once
 * per row). A suite whose cases are named differently by the two runners is a
 * suite whose results cannot be compared between them, so `.each` here is
 * uf's, registering one case per row through Bun's plain registration.
 * Mirrors `formatRow` in `../internal/registry.js`.
 */
function formatRow(name, row) {
  const values = Array.isArray(row) ? row : [row];
  let index = 0;
  return name.replace(ROW_TOKEN, (token) => {
    const value = values[index];
    index += 1;
    return token === "%j" ? (JSON.stringify(value) ?? "undefined") : String(value);
  });
}

/**
 * One of uf's registration functions, forwarding to Bun's.
 *
 * Forwarded call by call rather than re-exported, so that a modifier uf has
 * and Bun does not — `it.skipBecause` — can be attached without reaching into
 * Bun's own function object.
 */
function registration(bunApi) {
  const api = (...args) => bunApi(...args);
  api.only = (...args) => bunApi.only(...args);
  api.skip = (...args) => bunApi.skip(...args);
  api.todo = (...args) => bunApi.todo(...args);
  api.each = (table) => (name, body, options) => {
    for (const row of table) {
      bunApi(formatRow(name, row), () => body(row), options);
    }
  };
  return api;
}

export const describe = registration(bun.describe);
export const it = registration(bun.it);
it.skipBecause = refusing(
  "it.skipBecause",
  'Bun\'s skip carries no reason into its report; write `it.skip`, or run this file with `runner: "uf"`',
);
export const test = it;

/** uf's matchers that Bun's `expect` has no counterpart for. */
const MATCHERS_BUN_LACKS = [
  "toBeChecked",
  "toBeDisabled",
  "toBeEnabled",
  "toBeInTheDocument",
  "toBeRequired",
  "toBeVisible",
  "toHaveAttribute",
  "toHaveClass",
  "toHaveFocus",
  "toHaveTextContent",
  "toHaveValue",
  "toHaveNoAxeViolations",
];

bun.expect.extend(
  Object.fromEntries(
    MATCHERS_BUN_LACKS.map((matcher) => [
      matcher,
      refusing(
        `expect(…).${matcher}`,
        "Bun's `expect` has no such matcher; assert on the element's properties directly, or run this file with `runner: \"uf\"`",
      ),
    ]),
  ),
);

/**
 * uf's snapshot matchers.
 *
 * Bun has matchers by these names, and they are still not the same matchers:
 * both runners write `__snapshots__/<file>.snap` in Jest's format, but each
 * keys an entry by its own spelling of the test's name and serialises the value
 * its own way. Passed through, a suite recorded by uf would either fail against
 * its own snapshots or quietly write a second set beside them — and a snapshot
 * that was rewritten rather than compared is the failure snapshots exist to
 * catch.
 */
const SNAPSHOT_MATCHERS = ["toMatchSnapshot", "toMatchInlineSnapshot"];

bun.expect.extend(
  Object.fromEntries(
    SNAPSHOT_MATCHERS.map((matcher) => [
      matcher,
      refusing(
        `expect(…).${matcher}`,
        'uf and Bun key and serialise snapshots differently, so this would compare against — or rewrite — a snapshot uf did not write; run snapshot tests with `runner: "uf"`',
      ),
    ]),
  ),
);

export const expect = bun.expect;

export const fn = (...args) => bun.jest.fn(...args);
export const spyOn = (...args) => bun.spyOn(...args);

export const AssertionError = refusingClass(
  "AssertionError",
  "Bun throws its own assertion errors, so nothing a failure throws is an instance of uf's",
);
export const RunawayTimersError = refusingClass(
  "RunawayTimersError",
  "Bun's fake clock throws its own error for a timer loop that never ends",
);

/** A `uft` member with no equivalent under Bun. */
const refusingMember = (member, reason) => refusing(member, reason, UnsupportedError);

const MODULE_MOCKING =
  "Bun's `mock.module` rewrites modules that have already imported the real one, which is a different promise from `uft.mock`'s (see guide/testing, \"Which hosts\")";

const STUBS =
  "uf puts a stub back before the next file, and `bun test` runs every file in one process where nothing would";

export const uft = Object.freeze({
  fn,
  spyOn,
  mocked,

  clearAllMocks: () => bun.jest.clearAllMocks(),
  resetAllMocks: () => bun.jest.resetAllMocks(),
  restoreAllMocks: () => bun.jest.restoreAllMocks(),

  stubEnv: refusingMember("stubEnv", STUBS),
  unstubAllEnvs: refusingMember("unstubAllEnvs", STUBS),
  stubGlobal: refusingMember("stubGlobal", STUBS),
  unstubAllGlobals: refusingMember("unstubAllGlobals", STUBS),

  waitFor,
  waitUntil,

  useFakeTimers: () => {
    bun.jest.useFakeTimers();
  },
  useRealTimers: () => {
    bun.jest.useRealTimers();
  },
  isFakeTimers: () => bun.jest.isFakeTimers(),
  advanceTimersByTime: (milliseconds) => {
    bun.jest.advanceTimersByTime(milliseconds);
  },
  advanceTimersByTimeAsync: refusingMember(
    "advanceTimersByTimeAsync",
    "Bun's fake clock has no asynchronous advance; await between `uft.advanceTimersByTime` calls instead",
  ),
  advanceTimersToNextTimer: (steps) => {
    bun.jest.advanceTimersToNextTimer(steps);
  },
  runAllTimers: () => {
    bun.jest.runAllTimers();
  },
  runOnlyPendingTimers: () => {
    bun.jest.runOnlyPendingTimers();
  },
  getTimerCount: () => bun.jest.getTimerCount(),
  // uf's clock only moves while it is faked, and a time set before
  // `useFakeTimers` is forgotten when the clock is installed. Bun's
  // `setSystemTime` moves the real `Date` whether or not anything is faked, so
  // it is only forwarded while Bun's clock is faked — outside that it does
  // what uf's does, which is nothing a test can observe.
  setSystemTime: (time) => {
    if (bun.jest.isFakeTimers()) {
      bun.setSystemTime(time);
    }
  },
  getMockedSystemTime: () => (bun.jest.isFakeTimers() ? new Date() : null),

  mock: refusingMember("mock", MODULE_MOCKING),
  doMock: refusingMember("doMock", MODULE_MOCKING),
  unmock: refusingMember("unmock", MODULE_MOCKING),
  doUnmock: refusingMember("doUnmock", MODULE_MOCKING),
  importActual: refusingMember("importActual", MODULE_MOCKING),
  importMock: refusingMember("importMock", MODULE_MOCKING),
  resetModules: refusingMember("resetModules", MODULE_MOCKING),
});
