// @flow
//
// Collecting `describe` / `it` into a tree, and running it.
//
// A test file registers by being imported: `describe` runs its body
// immediately to collect children, `it` records a case. Nothing executes until
// the runner walks the tree afterwards, which is what makes `.only` decidable
// — a file's `.only` can appear after the tests it excludes.
//
// The rules, all of them the ones a person already expects:
//
// * `beforeEach` runs outermost-first and `afterEach` innermost-first, so a
//   suite's set-up wraps its children's.
// * `beforeAll` runs once before the first test in its suite that actually
//   runs, and `afterAll` after the last one — a suite whose tests are all
//   skipped never runs either, because there is nothing to set up for.
// * An `afterEach` runs even when the test failed, and its own failure is
//   reported rather than swallowed.
// * `.only` anywhere in the file restricts the file to marked cases and their
//   ancestors; everything else is reported skipped, never silently dropped.

import { callerSite, firstUserSite } from "./frames.js";

/** The placeholders `it.each` substitutes a row into. */
const ROW_TOKEN = /%[sjdi]/g;

/** What a test or hook body may return. */
export type Body = () => mixed | Promise<mixed>;

/**
 * One `beforeAll`, `afterAll`, `beforeEach` or `afterEach`.
 *
 * `timeoutMs` is the hook's own budget when its registration named one, and
 * `null` when the hook runs under the budget of the case it belongs to. A hook
 * that starts a process or a server needs longer than the cases it sets up for,
 * and without a budget of its own the only lever was the file-wide timeout.
 */
export type Hook = {| readonly body: Body, readonly timeoutMs: number | null |};

/** The suffix written on a registration call. */
export type Modifier = "none" | "only" | "skip" | "todo";

/**
 * How a benchmark runs under `uf test --bench`.
 *
 * `warmup` calls are made and their times thrown away, then `iterations` calls
 * are each timed. `timeout` is the budget for one call, in milliseconds, and
 * the benchmark as a whole is held to that budget once per call.
 */
export type BenchOptions = {|
  readonly warmup?: number,
  readonly iterations?: number,
  readonly timeout?: number,
|};

/** One registered test case, or one benchmark. */
export type Case = {|
  readonly kind: "test" | "bench",
  readonly name: string,
  readonly body: Body | null,
  readonly modifier: Modifier,
  readonly skipReason: string | null,
  readonly timeoutMs: number | null,
  /** How to run it, for a benchmark; `null` for a test. */
  readonly bench: BenchOptions | null,
  readonly line: number,
  readonly column: number,
|};

/** One `describe` and everything inside it. */
export type Suite = {|
  readonly kind: "suite",
  readonly name: string,
  readonly modifier: Modifier,
  readonly children: Array<Suite | Case>,
  readonly beforeAll: Array<Hook>,
  readonly afterAll: Array<Hook>,
  readonly beforeEach: Array<Hook>,
  readonly afterEach: Array<Hook>,
  readonly line: number,
  readonly column: number,
|};

function suite(name: string, modifier: Modifier, line: number, column: number): Suite {
  return {
    kind: "suite",
    name,
    modifier,
    children: [],
    beforeAll: [],
    afterAll: [],
    beforeEach: [],
    afterEach: [],
    line,
    column,
  };
}

/** The root suite of the file currently being collected. */
let root: Suite = suite("", "none", 0, 0);

/** The suite `describe`/`it` calls attach to right now. */
let current: Suite = root;

/** The names of the suites being registered into, outermost first. */
let path: Array<string> = [];

/** A position a registration was written at: one-based line and column. */
type Position = {| readonly line: number, readonly column: number |};

/**
 * Where discovery placed this file's declarations, by full name, or `null`
 * when `uf` said nothing.
 *
 * `uf` reads every `describe` and `it` off the source before it starts a
 * worker, so it already knows where each one was written, and it hands that
 * over with the request. Capturing a stack for each registration and mapping
 * it through a source map was about a fifth of a worker's CPU on a suite of
 * many small files, and it was the less reliable of the two: a module whose
 * transform moved lines, or whose map was not found, was placed on the wrong
 * line or on none. A name discovery could not pin down — made at run time,
 * declared twice, or carrying a modifier — is not in the table and is still
 * read off the stack.
 */
let known: Map<string, Position> | null = null;

/**
 * Take the positions `uf` sent with the request for the file about to be
 * imported. Anything but an object of `[line, column]` pairs is ignored, so a
 * worker handed nothing, or something it cannot read, falls back to the stack
 * for every registration.
 */
export function setKnownSites(sites: mixed): void {
  known = null;
  if (sites == null || typeof sites !== "object" || Array.isArray(sites)) {
    return;
  }
  const table = new Map<string, Position>();
  for (const [name, position] of Object.entries(sites)) {
    if (
      Array.isArray(position) &&
      typeof position[0] === "number" &&
      typeof position[1] === "number"
    ) {
      table.set(name, { line: position[0], column: position[1] });
    }
  }
  known = table;
}

/** The same separator the report and discovery join names with. */
const SEPARATOR = " > ";

/** Where the registration of `name`, in the suite being collected, was written. */
function siteOf(name: string): Position {
  if (known != null) {
    const full = [...path, name].filter((part) => part !== "").join(SEPARATOR);
    const found = known.get(full);
    if (found != null) {
      return found;
    }
  }
  return callSite();
}

/**
 * Start collecting a new file, discarding anything from the last one.
 *
 * The worker calls this before each import, so one file's registrations can
 * never leak into another's — which is the bug every "runner reuses a process"
 * design has to avoid.
 */
export function reset(): void {
  root = suite("", "none", 0, 0);
  current = root;
  path = [];
}

/** The tree collected since the last [`reset`]. */
export function collected(): Suite {
  return root;
}

/**
 * Where in the test file the call being registered was written.
 *
 * The stack is the only place this is available, and it is worth having: a
 * failure that names a line is a line a person can jump to. When the stack is
 * not in a shape we understand, the position is `0`, which every consumer
 * treats as "unknown" rather than as line one.
 */
function callSite(): {| readonly line: number, readonly column: number |} {
  const site = callerSite(callSite);
  if (site !== undefined) {
    return site ?? { line: 0, column: 0 };
  }
  return firstUserSite(new Error("position").stack) ?? { line: 0, column: 0 };
}

function addSuite(name: string, body: Body, modifier: Modifier): void {
  const position = modifier === "none" ? siteOf(name) : callSite();
  const child = suite(name, modifier, position.line, position.column);
  current.children.push(child);
  const parent = current;
  current = child;
  path.push(name);
  try {
    body();
  } finally {
    path.pop();
    current = parent;
  }
}

function addCase(
  name: string,
  body: Body | null,
  modifier: Modifier,
  timeoutMs: number | null,
  skipReason: string | null = null,
  bench: BenchOptions | null = null,
): void {
  const position = modifier === "none" ? siteOf(name) : callSite();
  current.children.push({
    kind: bench == null ? "test" : "bench",
    name,
    body,
    modifier,
    skipReason,
    timeoutMs,
    bench,
    line: position.line,
    column: position.column,
  });
}

/** Options a single test may carry. */
export type TestOptions = {| readonly timeout?: number |};

/**
 * The `describe` API, and its modifiers.
 *
 * The modifiers are properties on a callable, which is the shape every runner
 * has used since Jasmine and the one a person types without thinking. They are
 * attached inside this builder rather than assigned at the module's top level,
 * so importing this module still only *declares* things.
 */
function suiteApi(): $FlowFixMe {
  const api: $FlowFixMe = (name: string, body: Body) => {
    addSuite(name, body, "none");
  };
  api.only = (name: string, body: Body) => {
    addSuite(name, body, "only");
  };
  api.skip = (name: string, body: Body) => {
    addSuite(name, body, "skip");
  };
  api.todo = (name: string, body?: Body) => {
    addSuite(name, body ?? (() => {}), "todo");
  };
  api.each = (table: $ReadOnlyArray<mixed>) => (name: string, body: (row: mixed) => mixed) => {
    for (const row of table) {
      addSuite(formatRow(name, row), () => body(row), "none");
    }
  };
  return api;
}

/** The `it` API, and its modifiers. See [`suiteApi`] for the shape. */
function caseApi(): $FlowFixMe {
  const api: $FlowFixMe = (name: string, body: Body, options?: TestOptions) => {
    addCase(name, body, "none", options?.timeout ?? null);
  };
  api.only = (name: string, body: Body, options?: TestOptions) => {
    addCase(name, body, "only", options?.timeout ?? null);
  };
  api.skip = (name: string, body?: Body) => {
    addCase(name, body ?? null, "skip", null);
  };
  api.skipBecause = (name: string, reason: string, body?: Body) => {
    addCase(name, body ?? null, "skip", null, reason);
  };
  api.todo = (name: string, body?: Body) => {
    addCase(name, body ?? null, "todo", null);
  };
  api.each =
    (table: $ReadOnlyArray<mixed>) =>
    (name: string, body: (row: mixed) => mixed, options?: TestOptions) => {
      for (const row of table) {
        addCase(formatRow(name, row), () => body(row), "none", options?.timeout ?? null);
      }
    };
  return api;
}

/**
 * Group tests, and scope hooks to them.
 *
 * `describe.only`, `describe.skip` and `describe.todo` apply the modifier to
 * everything inside; `describe.each(table)` declares one suite per row.
 */
export const describe: $FlowFixMe = suiteApi();

/**
 * Register one test.
 *
 * `it.only`, `it.skip` and `it.todo` do what they say; `it.each(table)` runs
 * the body once per row, with `%s` and `%j` in the name replaced by the row.
 */
export const it: $FlowFixMe = caseApi();

/** `test` is `it`, for people who write it that way. */
export const test: $FlowFixMe = it;

/** The `bench` API, and its modifiers. See [`suiteApi`] for the shape. */
function benchApi(): $FlowFixMe {
  const api: $FlowFixMe = (name: string, body: Body, options?: BenchOptions) => {
    addCase(name, body, "none", options?.timeout ?? null, null, options ?? {});
  };
  api.only = (name: string, body: Body, options?: BenchOptions) => {
    addCase(name, body, "only", options?.timeout ?? null, null, options ?? {});
  };
  api.skip = (name: string, body?: Body) => {
    addCase(name, body ?? null, "skip", null, null, {});
  };
  api.todo = (name: string) => {
    addCase(name, null, "todo", null, null, {});
  };
  return api;
}

/**
 * Register one benchmark.
 *
 * `uf test` reports a benchmark as skipped, so a suite does not pay for timing
 * one, and `uf test --bench` runs the benchmarks in place of the tests and
 * reports how long each call took. `bench.only`, `bench.skip` and `bench.todo`
 * do what `it`'s do. See [`BenchOptions`] for `warmup`, `iterations` and
 * `timeout`.
 */
export const bench: $FlowFixMe = benchApi();

/**
 * Substitute a row into a name, the way every runner spells it: `%s` for the
 * value, `%j` for its JSON.
 */
function formatRow(name: string, row: mixed): string {
  const values = Array.isArray(row) ? row : [row];
  let index = 0;
  return name.replace(ROW_TOKEN, (token) => {
    const value = values[index];
    index += 1;
    return token === "%j" ? (JSON.stringify(value) ?? "undefined") : String(value);
  });
}

/**
 * A hook's own budget, from the second argument of its registration.
 *
 * `{ timeout }` is what `it` takes, and a bare number is what Jest and Vitest
 * take — a suite moved from either writes `beforeAll(start, 30_000)`, and a
 * budget that was silently dropped would fail as a timeout the file never
 * asked for.
 */
function hookTimeout(options: ?(TestOptions | number)): number | null {
  if (typeof options === "number") {
    return options;
  }
  return options?.timeout ?? null;
}

/**
 * Run once before the first test in this suite that runs.
 *
 * When it fails, every case it was setting up for fails with its error: none
 * of them runs against a setup that did not happen.
 */
export function beforeAll(body: Body, options?: TestOptions | number): void {
  current.beforeAll.push({ body, timeoutMs: hookTimeout(options) });
}

/** Run once after the last test in this suite that ran. */
export function afterAll(body: Body, options?: TestOptions | number): void {
  current.afterAll.push({ body, timeoutMs: hookTimeout(options) });
}

/** Run before every test in this suite and its children. */
export function beforeEach(body: Body, options?: TestOptions | number): void {
  current.beforeEach.push({ body, timeoutMs: hookTimeout(options) });
}

/** Run after every test in this suite and its children, including failures. */
export function afterEach(body: Body, options?: TestOptions | number): void {
  current.afterEach.push({ body, timeoutMs: hookTimeout(options) });
}
