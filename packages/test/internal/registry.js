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

import { firstUserSite } from "./frames.js";

/** The placeholders `it.each` substitutes a row into. */
const ROW_TOKEN = /%[sjdi]/g;

/** What a test or hook body may return. */
export type Body = () => mixed | Promise<mixed>;

/** The suffix written on a registration call. */
export type Modifier = "none" | "only" | "skip" | "todo";

/** One registered test case. */
export type Case = {|
  readonly kind: "test",
  readonly name: string,
  readonly body: Body | null,
  readonly modifier: Modifier,
  readonly timeoutMs: number | null,
  readonly line: number,
  readonly column: number,
|};

/** One `describe` and everything inside it. */
export type Suite = {|
  readonly kind: "suite",
  readonly name: string,
  readonly modifier: Modifier,
  readonly children: Array<Suite | Case>,
  readonly beforeAll: Array<Body>,
  readonly afterAll: Array<Body>,
  readonly beforeEach: Array<Body>,
  readonly afterEach: Array<Body>,
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
  return firstUserSite(new Error("position").stack) ?? { line: 0, column: 0 };
}

function addSuite(name: string, body: Body, modifier: Modifier): void {
  const position = callSite();
  const child = suite(name, modifier, position.line, position.column);
  current.children.push(child);
  const parent = current;
  current = child;
  try {
    body();
  } finally {
    current = parent;
  }
}

function addCase(
  name: string,
  body: Body | null,
  modifier: Modifier,
  timeoutMs: number | null,
): void {
  const position = callSite();
  current.children.push({
    kind: "test",
    name,
    body,
    modifier,
    timeoutMs,
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
    return token === "%j" ? JSON.stringify(value) ?? "undefined" : String(value);
  });
}

/** Run once before the first test in this suite that runs. */
export function beforeAll(body: Body): void {
  current.beforeAll.push(body);
}

/** Run once after the last test in this suite that ran. */
export function afterAll(body: Body): void {
  current.afterAll.push(body);
}

/** Run before every test in this suite and its children. */
export function beforeEach(body: Body): void {
  current.beforeEach.push(body);
}

/** Run after every test in this suite and its children, including failures. */
export function afterEach(body: Body): void {
  current.afterEach.push(body);
}
