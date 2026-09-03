// @flow
//
// `expect`, and the matchers it carries.
//
// A failed matcher throws an `AssertionError` whose message already says what
// was wanted and what arrived; the runner never reformats it, so what a person
// reads is what the matcher decided to say. Every matcher works under `.not`
// without being written twice: a matcher returns a verdict carrying both
// messages, and negation chooses the other one.
//
// `.resolves` and `.rejects` settle the promise first and then apply the same
// matcher table to what came out, so `await expect(p).resolves.toBe(1)` reads
// the way the synchronous form does.

import { equals, matchesObject, render } from "./equality.js";

/** Thrown when a matcher does not hold. */
export class AssertionError extends Error {
  /** What the assertion wanted, rendered. */
  expected: string;
  /** What arrived, rendered. */
  received: string;
  /** The matcher's name, e.g. `toEqual`. */
  matcher: string;

  constructor(message: string, matcher: string, expected: string, received: string) {
    super(message);
    this.name = "AssertionError";
    this.matcher = matcher;
    this.expected = expected;
    this.received = received;
  }
}

/** What a matcher decided, and how to say it either way. */
type Verdict = {|
  +pass: boolean,
  +failure: () => string,
  +negatedFailure: () => string,
  +expected?: string,
  +received?: string,
|};

/** One recorded call to a spy. */
export type SpyCall = {|
  +args: $ReadOnlyArray<mixed>,
  +returned?: mixed,
  +threw?: mixed,
|};

/**
 * A spy that records its calls.
 *
 * `fn()` records and returns `undefined`; `fn(implementation)` records and
 * delegates. A throw is recorded and then re-thrown, so wrapping a function in
 * a spy never changes whether the code under test fails.
 */
export function fn(implementation?: (...args: $ReadOnlyArray<mixed>) => mixed): $FlowFixMe {
  const calls: Array<SpyCall> = [];
  let current = implementation;

  const spy: $FlowFixMe = (...args: $ReadOnlyArray<mixed>) => {
    try {
      const returned = current == null ? undefined : current(...args);
      calls.push({ args, returned });
      return returned;
    } catch (thrown) {
      calls.push({ args, threw: thrown });
      throw thrown;
    }
  };
  spy.mock = { calls };
  spy.mockClear = () => {
    calls.length = 0;
  };
  spy.mockReturnValue = (value: mixed) => {
    current = () => value;
    return spy;
  };
  spy.mockResolvedValue = (value: mixed) => {
    current = () => Promise.resolve(value);
    return spy;
  };
  spy.mockRejectedValue = (reason: mixed) => {
    current = () => Promise.reject(reason);
    return spy;
  };
  spy.mockImplementation = (next: (...args: $ReadOnlyArray<mixed>) => mixed) => {
    current = next;
    return spy;
  };
  return spy;
}

function isSpy(value: mixed): boolean {
  return typeof value === "function" && (value: $FlowFixMe).mock != null;
}

function propertyAt(value: mixed, path: string): {| +found: boolean, +value: mixed |} {
  let current = value;
  for (const key of path.split(".")) {
    if (current == null) {
      return { found: false, value: undefined };
    }
    if (!Object.prototype.hasOwnProperty.call((current: $FlowFixMe), key)) {
      return { found: false, value: undefined };
    }
    current = (current: $FlowFixMe)[key];
  }
  return { found: true, value: current };
}

function describeThrown(thrown: mixed): string {
  return thrown instanceof Error ? `${thrown.name}: ${thrown.message}` : render(thrown);
}

function matchesThrown(thrown: mixed, expected: mixed): boolean {
  const message = thrown instanceof Error ? thrown.message : String(thrown);
  if (typeof expected === "string") {
    return message.includes(expected);
  }
  if (expected instanceof RegExp) {
    return expected.test(message);
  }
  if (expected instanceof Error) {
    return message === expected.message;
  }
  if (typeof expected === "function") {
    return thrown instanceof (expected: $FlowFixMe);
  }
  return equals(thrown, expected);
}

/**
 * The matcher table for one received value.
 *
 * Every entry returns a [`Verdict`] rather than throwing, which is what lets
 * `.not` reuse all of them.
 */
function verdicts(received: mixed): { +[string]: (...args: $ReadOnlyArray<any>) => Verdict } {
  const shown = () => render(received);
  const simple = (pass: boolean, what: string, expected?: mixed): Verdict => ({
    pass,
    expected: expected === undefined ? what : render(expected),
    received: shown(),
    failure: () => `expected ${shown()} ${what}`,
    negatedFailure: () => `expected ${shown()} not ${what}`,
  });
  const spyCalls = (): Array<SpyCall> => (isSpy(received) ? (received: $FlowFixMe).mock.calls : []);
  const requireSpy = (matcher: string) => {
    if (!isSpy(received)) {
      throw new AssertionError(
        `${matcher} needs a spy made by \`fn()\`, but received ${shown()}`,
        matcher,
        "a spy",
        shown(),
      );
    }
  };

  return {
    toBe: (expected: mixed) => ({
      pass: Object.is(received, expected),
      expected: render(expected),
      received: shown(),
      failure: () => `expected ${shown()} to be ${render(expected)}`,
      negatedFailure: () => `expected ${shown()} not to be ${render(expected)}`,
    }),
    toEqual: (expected: mixed) => ({
      pass: equals(received, expected, [], "loose"),
      expected: render(expected),
      received: shown(),
      failure: () => `expected ${shown()} to equal ${render(expected)}`,
      negatedFailure: () => `expected ${shown()} not to equal ${render(expected)}`,
    }),
    toStrictEqual: (expected: mixed) => ({
      pass: equals(received, expected, [], "strict"),
      expected: render(expected),
      received: shown(),
      failure: () => `expected ${shown()} to strictly equal ${render(expected)}`,
      negatedFailure: () => `expected ${shown()} not to strictly equal ${render(expected)}`,
    }),
    toBeTruthy: () => simple(Boolean(received), "to be truthy"),
    toBeFalsy: () => simple(!received, "to be falsy"),
    toBeNull: () => simple(received === null, "to be null"),
    toBeUndefined: () => simple(received === undefined, "to be undefined"),
    toBeDefined: () => simple(received !== undefined, "to be defined"),
    toBeNaN: () => simple(typeof received === "number" && Number.isNaN(received), "to be NaN"),
    toBeGreaterThan: (expected: mixed) =>
      simple((received: $FlowFixMe) > (expected: $FlowFixMe), `to be greater than ${render(expected)}`, expected),
    toBeGreaterThanOrEqual: (expected: mixed) =>
      simple((received: $FlowFixMe) >= (expected: $FlowFixMe), `to be at least ${render(expected)}`, expected),
    toBeLessThan: (expected: mixed) =>
      simple((received: $FlowFixMe) < (expected: $FlowFixMe), `to be less than ${render(expected)}`, expected),
    toBeLessThanOrEqual: (expected: mixed) =>
      simple((received: $FlowFixMe) <= (expected: $FlowFixMe), `to be at most ${render(expected)}`, expected),
    toBeCloseTo: (expected: number, digits?: number) => {
      const places = digits ?? 2;
      const tolerance = 10 ** -places / 2;
      const difference = Math.abs((received: $FlowFixMe) - expected);
      return simple(
        difference < tolerance,
        `to be within ${tolerance} of ${expected}, but it is off by ${difference}`,
        expected,
      );
    },
    toContain: (expected: mixed) => {
      const pass =
        typeof received === "string"
          ? received.includes(String(expected))
          : Array.isArray(received)
            ? received.some((item) => Object.is(item, expected))
            : received instanceof Set
              ? received.has(expected)
              : false;
      return simple(pass, `to contain ${render(expected)}`, expected);
    },
    toContainEqual: (expected: mixed) => {
      const items = Array.isArray(received) ? received : received instanceof Set ? [...received] : [];
      return simple(
        items.some((item) => equals(item, expected)),
        `to contain something equal to ${render(expected)}`,
        expected,
      );
    },
    toHaveLength: (expected: number) => {
      const length = received == null ? undefined : (received: $FlowFixMe).length;
      return simple(length === expected, `to have length ${expected}, not ${render(length)}`, expected);
    },
    toHaveProperty: (path: string, ...rest: $ReadOnlyArray<mixed>) => {
      const found = propertyAt(received, path);
      if (rest.length === 0) {
        return simple(found.found, `to have a property at \`${path}\``);
      }
      return simple(
        found.found && equals(found.value, rest[0]),
        `to have \`${path}\` equal to ${render(rest[0])}, not ${render(found.value)}`,
        rest[0],
      );
    },
    toMatch: (expected: mixed) => {
      const text = typeof received === "string" ? received : String(received);
      const pass = typeof expected === "string" ? text.includes(expected) : (expected: $FlowFixMe).test(text);
      return simple(pass, `to match ${render(expected)}`, expected);
    },
    toMatchObject: (expected: mixed) =>
      simple(matchesObject(received, expected), `to match ${render(expected)}`, expected),
    toBeInstanceOf: (expected: mixed) =>
      simple(
        typeof expected === "function" && received instanceof (expected: $FlowFixMe),
        `to be an instance of ${render(expected)}`,
        expected,
      ),
    toBeTypeOf: (expected: string) =>
      simple(typeof received === expected, `to be of type ${expected}, not ${typeof received}`, expected),
    toSatisfy: (predicate: (value: mixed) => boolean) =>
      simple(predicate(received) === true, "to satisfy the predicate"),
    toThrow: (...rest: $ReadOnlyArray<mixed>) => {
      const expected = rest[0];
      if (typeof received !== "function") {
        return simple(false, "to be a function, so it could be called");
      }
      let thrown: mixed;
      let threw = false;
      try {
        received();
      } catch (error) {
        threw = true;
        thrown = error;
      }
      if (!threw) {
        return {
          pass: false,
          expected: rest.length === 0 ? "a throw" : render(expected),
          received: "no throw",
          failure: () => "expected the function to throw, but it returned",
          negatedFailure: () => "expected the function not to throw",
        };
      }
      return {
        pass: rest.length === 0 || matchesThrown(thrown, expected),
        expected: rest.length === 0 ? "a throw" : render(expected),
        received: describeThrown(thrown),
        failure: () =>
          `expected the function to throw ${render(expected)}, but it threw ${describeThrown(thrown)}`,
        negatedFailure: () => `expected the function not to throw ${describeThrown(thrown)}`,
      };
    },
    toHaveBeenCalled: () => {
      requireSpy("toHaveBeenCalled");
      return simple(spyCalls().length > 0, "to have been called");
    },
    toHaveBeenCalledTimes: (count: number) => {
      requireSpy("toHaveBeenCalledTimes");
      const actual = spyCalls().length;
      return simple(actual === count, `to have been called ${count} times, not ${actual}`, count);
    },
    toHaveBeenCalledWith: (...args: $ReadOnlyArray<mixed>) => {
      requireSpy("toHaveBeenCalledWith");
      const calls = spyCalls();
      return simple(
        calls.some((call) => equals([...call.args], [...args])),
        `to have been called with ${render(args)}; the calls were ${render(calls.map((call) => call.args))}`,
        args,
      );
    },
    toHaveBeenLastCalledWith: (...args: $ReadOnlyArray<mixed>) => {
      requireSpy("toHaveBeenLastCalledWith");
      const calls = spyCalls();
      const last = calls.length === 0 ? undefined : calls[calls.length - 1];
      return simple(
        last != null && equals([...last.args], [...args]),
        `to have last been called with ${render(args)}, not ${render(last == null ? undefined : last.args)}`,
        args,
      );
    },
  };
}

/**
 * Turn the verdict table into the object a caller uses.
 *
 * `negated` decides which message a failing verdict raises, which is all of
 * what `.not` is.
 */
function bind(received: mixed, negated: boolean): $FlowFixMe {
  const table = verdicts(received);
  const bound: $FlowFixMe = {};
  for (const name of Object.keys(table)) {
    bound[name] = (...args: $ReadOnlyArray<mixed>) => {
      const verdict = table[name](...args);
      if (verdict.pass !== negated) {
        return undefined;
      }
      const message = negated ? verdict.negatedFailure() : verdict.failure();
      throw new AssertionError(message, name, verdict.expected ?? "", verdict.received ?? render(received));
    };
  }
  Object.defineProperty(bound, "not", { get: () => bind(received, !negated) });
  return bound;
}

/**
 * The `.resolves` / `.rejects` surface: settle the promise, then apply the
 * same matcher to what came out.
 */
function settled(promise: mixed, wanted: "resolve" | "reject", negated: boolean): $FlowFixMe {
  const bound: $FlowFixMe = {};
  for (const name of Object.keys(verdicts(undefined))) {
    bound[name] = async (...args: $ReadOnlyArray<mixed>) => {
      let value: mixed;
      let rejected = false;
      try {
        value = await (promise: $FlowFixMe);
      } catch (error) {
        rejected = true;
        value = error;
      }
      if (wanted === "resolve" && rejected) {
        throw new AssertionError(
          `expected the promise to resolve, but it rejected with ${describeThrown(value)}`,
          name,
          "a resolved promise",
          describeThrown(value),
        );
      }
      if (wanted === "reject" && !rejected) {
        throw new AssertionError(
          `expected the promise to reject, but it resolved with ${render(value)}`,
          name,
          "a rejected promise",
          render(value),
        );
      }
      // `toThrow` reads a function and calls it, but a settled promise has
      // already produced its reason. Handing the matcher a thunk that throws
      // that reason is what makes `.rejects.toThrow(/nope/)` mean what it
      // plainly says, with one matcher rather than two.
      const subject =
        name === "toThrow" && typeof value !== "function"
          ? () => {
              throw value;
            }
          : value;
      bind(subject, negated)[name](...args);
    };
  }
  Object.defineProperty(bound, "not", { get: () => settled(promise, wanted, !negated) });
  return bound;
}

/**
 * Assert on `received`.
 *
 * ```js
 * expect(sum(2, 2)).toBe(4);
 * expect(user).toMatchObject({ name: "ada" });
 * expect(() => parse("")).toThrow(/empty/);
 * await expect(load()).resolves.toHaveLength(3);
 * ```
 */
export function expect(received: mixed): $FlowFixMe {
  const expectation: $FlowFixMe = bind(received, false);
  Object.defineProperty(expectation, "resolves", { get: () => settled(received, "resolve", false) });
  Object.defineProperty(expectation, "rejects", { get: () => settled(received, "reject", false) });
  return expectation;
}
