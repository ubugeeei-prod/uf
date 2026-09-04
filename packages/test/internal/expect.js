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
  readonly pass: boolean,
  readonly failure: () => string,
  readonly negatedFailure: () => string,
  readonly expected?: string,
  readonly received?: string,
|};

/** One recorded call to a spy. */
export type SpyCall = {|
  readonly args: $ReadOnlyArray<mixed>,
  readonly returned?: mixed,
  readonly threw?: mixed,
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
  return typeof value === "function" && (value as $FlowFixMe).mock != null;
}

function propertyAt(
  value: mixed,
  path: string,
): {| readonly found: boolean, readonly value: mixed |} {
  let current = value;
  for (const key of path.split(".")) {
    if (current == null) {
      return { found: false, value: undefined };
    }
    if (!Object.prototype.hasOwnProperty.call(current as $FlowFixMe, key)) {
      return { found: false, value: undefined };
    }
    current = (current as $FlowFixMe)[key];
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
    return thrown instanceof (expected as $FlowFixMe);
  }
  return equals(thrown, expected);
}

/**
 * The matcher table for one received value.
 *
 * Every entry returns a [`Verdict`] rather than throwing, which is what lets
 * `.not` reuse all of them.
 */
function verdicts(received: mixed): {
  readonly [string]: (...args: $ReadOnlyArray<any>) => Verdict,
} {
  const shown = () => render(received);
  const simple = (pass: boolean, what: string, expected?: mixed): Verdict => ({
    pass,
    expected: expected === undefined ? what : render(expected),
    received: shown(),
    failure: () => `expected ${shown()} ${what}`,
    negatedFailure: () => `expected ${shown()} not ${what}`,
  });
  const spyCalls = (): Array<SpyCall> =>
    isSpy(received) ? (received as $FlowFixMe).mock.calls : [];
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
      simple(
        (received as $FlowFixMe) > (expected as $FlowFixMe),
        `to be greater than ${render(expected)}`,
        expected,
      ),
    toBeGreaterThanOrEqual: (expected: mixed) =>
      simple(
        (received as $FlowFixMe) >= (expected as $FlowFixMe),
        `to be at least ${render(expected)}`,
        expected,
      ),
    toBeLessThan: (expected: mixed) =>
      simple(
        (received as $FlowFixMe) < (expected as $FlowFixMe),
        `to be less than ${render(expected)}`,
        expected,
      ),
    toBeLessThanOrEqual: (expected: mixed) =>
      simple(
        (received as $FlowFixMe) <= (expected as $FlowFixMe),
        `to be at most ${render(expected)}`,
        expected,
      ),
    toBeCloseTo: (expected: number, digits?: number) => {
      const places = digits ?? 2;
      const tolerance = 10 ** -places / 2;
      const difference = Math.abs((received as $FlowFixMe) - expected);
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
      const items = Array.isArray(received)
        ? received
        : received instanceof Set
          ? [...received]
          : [];
      return simple(
        items.some((item) => equals(item, expected)),
        `to contain something equal to ${render(expected)}`,
        expected,
      );
    },
    toHaveLength: (expected: number) => {
      const length = received == null ? undefined : (received as $FlowFixMe).length;
      return simple(
        length === expected,
        `to have length ${expected}, not ${render(length)}`,
        expected,
      );
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
      const pass =
        typeof expected === "string"
          ? text.includes(expected)
          : (expected as $FlowFixMe).test(text);
      return simple(pass, `to match ${render(expected)}`, expected);
    },
    toMatchObject: (expected: mixed) =>
      simple(matchesObject(received, expected), `to match ${render(expected)}`, expected),
    toBeInstanceOf: (expected: mixed) =>
      simple(
        typeof expected === "function" && received instanceof (expected as $FlowFixMe),
        `to be an instance of ${render(expected)}`,
        expected,
      ),
    toBeTypeOf: (expected: string) =>
      simple(
        typeof received === expected,
        `to be of type ${expected}, not ${typeof received}`,
        expected,
      ),
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

    // ---------------------------------------------------------------- //
    // Elements
    //
    // These read properties of whatever they are given, so this module still
    // needs no DOM and no dependency on one: an element is an object with a
    // `tagName`, and a process without a document simply never has one to
    // pass in. `element` says so when it does not.
    // ---------------------------------------------------------------- //

    toBeInTheDocument: () => {
      const node = element("toBeInTheDocument");
      const root = node.ownerDocument;
      return simple(root != null && root.contains(node), "to be in the document");
    },
    toBeVisible: () => {
      const node = element("toBeVisible");
      return simple(isVisible(node), "to be visible");
    },
    toBeDisabled: () => {
      const node = element("toBeDisabled");
      return simple(isDisabled(node), "to be disabled");
    },
    toBeEnabled: () => {
      const node = element("toBeEnabled");
      return simple(!isDisabled(node), "to be enabled");
    },
    toBeChecked: () => {
      const node = element("toBeChecked");
      const aria = node.getAttribute("aria-checked");
      const checked = aria != null ? aria === "true" : (node as $FlowFixMe).checked === true;
      return simple(checked, "to be checked");
    },
    toBeRequired: () => {
      const node = element("toBeRequired");
      return simple(
        (node as $FlowFixMe).required === true || node.getAttribute("aria-required") === "true",
        "to be required",
      );
    },
    toHaveFocus: () => {
      const node = element("toHaveFocus");
      return simple(node.ownerDocument?.activeElement === node, "to have focus");
    },
    toHaveAttribute: (name: mixed, value?: mixed) => {
      const node = element("toHaveAttribute");
      const actual = node.getAttribute(String(name));
      if (value === undefined) {
        return simple(actual != null, `to have the attribute ${render(name)}`, name);
      }
      return {
        pass: actual === String(value),
        expected: render(value),
        received: render(actual),
        failure: () => `expected ${render(name)} to be ${render(value)}, not ${render(actual)}`,
      };
    },
    toHaveClass: (...names: $ReadOnlyArray<mixed>) => {
      const node = element("toHaveClass");
      const classes = (node.getAttribute("class") ?? "").split(/\s+/).filter(Boolean);
      const wanted = names.map(String);
      return {
        pass: wanted.every((name) => classes.includes(name)),
        expected: render(wanted),
        received: render(classes),
        failure: () => `expected the class list ${render(classes)} to include ${render(wanted)}`,
      };
    },
    toHaveTextContent: (expected: mixed) => {
      const node = element("toHaveTextContent");
      const text = (node.textContent ?? "").replace(/\s+/g, " ").trim();
      const pass =
        expected instanceof RegExp ? expected.test(text) : text.includes(String(expected));
      return {
        pass,
        expected: render(expected),
        received: render(text),
        failure: () => `expected the text ${render(text)} to contain ${render(expected)}`,
      };
    },
    toHaveValue: (expected: mixed) => {
      const node = element("toHaveValue");
      const actual = (node as $FlowFixMe).value;
      return {
        pass: equals(actual, expected),
        expected: render(expected),
        received: render(actual),
        failure: () => `expected the value ${render(actual)} to be ${render(expected)}`,
      };
    },
  };

  /**
   * The received value as an element, or a failure that says what it was.
   *
   * An element matcher applied to a string is almost always a query whose
   * result was used without being awaited, and "received a Promise" is a much
   * better message than a `TypeError` about `getAttribute`.
   */
  function element(matcher: string): Element {
    const node: $FlowFixMe = received;
    if (node == null || typeof node.getAttribute !== "function") {
      throw new AssertionError(`${matcher} needs an element, and received ${render(received)}`);
    }
    return node;
  }
}

/**
 * Whether a reader would see this element.
 *
 * Walks the ancestors, because `display: none` on a parent hides a child whose
 * own style says nothing. `hidden`, `aria-hidden` and a `details` that is not
 * open each hide their subtree too.
 */
function isVisible(node: Element): boolean {
  let current: $FlowFixMe = node;
  while (current != null && current.nodeType === 1) {
    if (current.hasAttribute("hidden") || current.getAttribute("aria-hidden") === "true") {
      return false;
    }
    if (current.tagName === "DETAILS" && !current.hasAttribute("open") && current !== node) {
      return false;
    }
    const style = current.ownerDocument?.defaultView?.getComputedStyle?.(current);
    if (style != null) {
      if (style.display === "none" || style.visibility === "hidden") {
        return false;
      }
      if (style.opacity === "0") {
        return false;
      }
    }
    current = current.parentElement;
  }
  return true;
}

/** Whether the control is disabled, by its own attribute or a fieldset's. */
function isDisabled(node: Element): boolean {
  let current: $FlowFixMe = node;
  while (current != null && current.nodeType === 1) {
    if (current.hasAttribute("disabled")) {
      return true;
    }
    if (current.getAttribute("aria-disabled") === "true") {
      return true;
    }
    current = current.parentElement;
  }
  return false;
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
      throw new AssertionError(
        message,
        name,
        verdict.expected ?? "",
        verdict.received ?? render(received),
      );
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
        value = await (promise as $FlowFixMe);
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
  Object.defineProperty(expectation, "resolves", {
    get: () => settled(received, "resolve", false),
  });
  Object.defineProperty(expectation, "rejects", { get: () => settled(received, "reject", false) });
  return expectation;
}
