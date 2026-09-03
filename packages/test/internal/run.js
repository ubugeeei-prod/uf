// @flow
//
// Walking a collected suite tree and executing it.
//
// This is the half of the runner that knows what a test *is*; the half that
// knows about processes, scheduling and terminals is `uf_test` in Rust. The
// split is deliberate: everything here is pure with respect to the host — it
// takes a tree and an `emit` callback and returns nothing — so it can be
// driven by the worker, by a unit test, or by a future host that is not
// Node.js, without any of them re-deciding what `.only` means.

import { AssertionError } from "./expect.js";
import { firstUserSite, userFrames } from "./frames.js";
import { type Body, type Case, type Suite, collected } from "./registry.js";

/** How one case ended. */
export type Outcome =
  | {| +status: "passed" |}
  | {|
      +status: "failed",
      +message: string,
      +stack: string | null,
      +expected: string | null,
      +received: string | null,
      /** Where the failing assertion was written, when the stack says. */
      +site: {| +line: number, +column: number |} | null,
    |}
  | {| +status: "skipped", +reason: "explicit" | "not-only" | "filtered" |}
  | {| +status: "todo" |};

/** One finished case, as the runner reports it. */
export type Result = {|
  +name: string,
  +line: number,
  +column: number,
  +durationMicros: number,
  +outcome: Outcome,
|};

/** How a run is configured. */
export type RunOptions = {|
  /** Keep only cases whose full name contains this, reporting the rest skipped. */
  +filter?: string | null,
  /** Wall-clock budget for one case, in milliseconds. */
  +timeoutMs?: number,
|};

/** Default budget for one case, matching what most runners use. */
export const DEFAULT_TIMEOUT_MS: number = 5000;

/** The separator between a suite's name and its child's. */
export const NAME_SEPARATOR: string = " > ";

function fullName(path: $ReadOnlyArray<string>): string {
  return path.filter((part) => part !== "").join(NAME_SEPARATOR);
}

/**
 * Whether the tree contains a case marked `.only`, directly or through a
 * suite marked `.only`.
 *
 * `.only` is per file, and this is the question that makes it so.
 */
function hasOnly(node: Suite | Case, inherited: boolean): boolean {
  const marked = inherited || node.modifier === "only";
  if (node.kind === "test") {
    return marked;
  }
  return node.children.some((child) => hasOnly(child, marked));
}

/**
 * Run `body` with a wall-clock budget.
 *
 * A body that never settles must not hang the whole run, and there is no way
 * to interrupt JavaScript, so the budget is a race: the run continues and the
 * case is reported as timed out. The abandoned work may still be running,
 * which is why the worker is torn down between files.
 */
async function withTimeout(body: Body, timeoutMs: number): Promise<void> {
  let timer: TimeoutID | null = null;
  const timeout = new Promise<empty>((_resolve, reject) => {
    timer = setTimeout(() => {
      reject(new Error(`timed out after ${timeoutMs}ms`));
    }, timeoutMs);
  });
  try {
    await Promise.race([Promise.resolve().then(body), timeout]);
  } finally {
    if (timer != null) {
      clearTimeout(timer);
    }
  }
}

function failure(thrown: mixed): Outcome {
  if (thrown instanceof AssertionError) {
    const stack = userFrames(thrown.stack);
    return {
      status: "failed",
      message: thrown.message,
      stack,
      expected: thrown.expected,
      received: thrown.received,
      site: firstUserSite(stack, false),
    };
  }
  if (thrown instanceof Error) {
    const stack = userFrames(thrown.stack);
    return {
      status: "failed",
      message: `${thrown.name}: ${thrown.message}`,
      stack,
      expected: null,
      received: null,
      site: firstUserSite(stack, false),
    };
  }
  return {
    status: "failed",
    message: `the test threw ${String(thrown)}`,
    stack: null,
    expected: null,
    received: null,
    site: null,
  };
}

/** Everything one case needs from the suites above it. */
type Context = {|
  +path: $ReadOnlyArray<string>,
  +beforeEach: $ReadOnlyArray<Body>,
  +afterEach: $ReadOnlyArray<Body>,
  +skipped: boolean,
  +onlyPath: boolean,
|};

/**
 * Execute one case, hooks included.
 *
 * `afterEach` runs even when the body failed, and a hook's own failure is
 * reported rather than replacing the body's — the first failure wins, because
 * it is the one that explains the rest.
 */
async function runCase(
  test: Case,
  context: Context,
  options: RunOptions,
  emit: (result: Result) => void,
): Promise<boolean> {
  const name = fullName([...context.path, test.name]);
  const started = performance.now();
  const report = (outcome: Outcome) => {
    emit({
      name,
      line: test.line,
      column: test.column,
      durationMicros: Math.round((performance.now() - started) * 1000),
      outcome,
    });
  };

  if (test.modifier === "todo" || test.body == null) {
    report({ status: "todo" });
    return true;
  }
  if (context.skipped || test.modifier === "skip") {
    report({ status: "skipped", reason: "explicit" });
    return true;
  }
  if (!context.onlyPath) {
    report({ status: "skipped", reason: "not-only" });
    return true;
  }
  const filter = options.filter;
  if (filter != null && filter !== "" && !name.includes(filter)) {
    report({ status: "skipped", reason: "filtered" });
    return true;
  }

  const timeoutMs = test.timeoutMs ?? options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  let outcome: Outcome = { status: "passed" };
  try {
    for (const hook of context.beforeEach) {
      await withTimeout(hook, timeoutMs);
    }
    await withTimeout(test.body, timeoutMs);
  } catch (thrown) {
    outcome = failure(thrown);
  }
  // Teardown runs whatever happened above, and only reports its own failure
  // when the body had not already failed.
  for (const hook of context.afterEach) {
    try {
      await withTimeout(hook, timeoutMs);
    } catch (thrown) {
      if (outcome.status === "passed") {
        outcome = failure(thrown);
      }
    }
  }
  report(outcome);
  return outcome.status !== "failed";
}

/**
 * Walk one suite, running what it contains.
 *
 * Returns whether everything under it passed, which is what `bail` reads.
 */
async function runSuite(
  node: Suite,
  context: Context,
  options: RunOptions,
  onlyMode: boolean,
  emit: (result: Result) => void,
  state: {| bail: boolean |},
): Promise<boolean> {
  const skipped = context.skipped || node.modifier === "skip" || node.modifier === "todo";
  const onlyPath = !onlyMode || context.onlyPath || node.modifier === "only";
  const path = node.name === "" ? context.path : [...context.path, node.name];
  const inner: Context = {
    path,
    beforeEach: [...context.beforeEach, ...node.beforeEach],
    afterEach: [...node.afterEach, ...context.afterEach],
    skipped,
    onlyPath,
  };

  // `beforeAll` is deferred until a case in this suite actually runs, so a
  // fully skipped suite never sets anything up. `afterAll` mirrors it.
  let setUp = false;
  const setUpOnce = async () => {
    if (setUp) {
      return;
    }
    setUp = true;
    for (const hook of node.beforeAll) {
      await withTimeout(hook, options.timeoutMs ?? DEFAULT_TIMEOUT_MS);
    }
  };

  let passed = true;
  for (const child of node.children) {
    if (state.bail) {
      break;
    }
    if (child.kind === "test") {
      const willRun =
        !inner.skipped &&
        child.modifier !== "skip" &&
        child.modifier !== "todo" &&
        child.body != null &&
        (!onlyMode || inner.onlyPath || child.modifier === "only");
      if (willRun) {
        try {
          await setUpOnce();
        } catch (thrown) {
          // A failed `beforeAll` fails the cases it was setting up for, named
          // as such: reporting the hook alone would leave the tests silent.
          emit({
            name: fullName([...inner.path, child.name]),
            line: child.line,
            column: child.column,
            durationMicros: 0,
            outcome: failure(thrown),
          });
          passed = false;
          continue;
        }
      }
      const childContext: Context = {
        ...inner,
        onlyPath: !onlyMode || inner.onlyPath || child.modifier === "only",
      };
      const ok = await runCase(child, childContext, options, emit);
      passed = passed && ok;
    } else {
      const ok = await runSuite(child, inner, options, onlyMode, emit, state);
      passed = passed && ok;
    }
  }

  if (setUp) {
    for (const hook of node.afterAll) {
      try {
        await withTimeout(hook, options.timeoutMs ?? DEFAULT_TIMEOUT_MS);
      } catch {
        // A teardown failure cannot fail a test that already reported, and
        // there is nothing left to attach it to; the file's own status carries
        // it instead, which the worker reports.
        passed = false;
      }
    }
  }
  return passed;
}

/**
 * Run the tree collected since the last `reset`, reporting each case through
 * `emit` as it finishes.
 */
export async function run(options: RunOptions, emit: (result: Result) => void): Promise<void> {
  const root = collected();
  const onlyMode = hasOnly(root, false);
  const context: Context = {
    path: [],
    beforeEach: [],
    afterEach: [],
    skipped: false,
    onlyPath: !onlyMode,
  };
  await runSuite(root, context, options, onlyMode, emit, { bail: false });
}
