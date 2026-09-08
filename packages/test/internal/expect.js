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
//
// # The names are written down, and the behaviour is not
//
// `expect` used to be `$FlowFixMe`, and so was everything it handed back. That
// is a hole in the published type of the package a project writes every one of
// its assertions against: `expect(user).toBaa(1)` was not a misspelling
// anybody's checker would find, `expect(list).toHaveLength("3")` was not a type
// error, and `expect(p).resolves` on a value that is not a promise was fine
// until it ran. Every test in this repository is written against `expect`,
// which is the largest surface in the packages and was the least checked.
//
// So [`Matchers`] below writes the names out, one signature per matcher, the
// way `@uniflowed/react-testing`'s `Queries` writes out its thirty-six. Flow
// has no template literal types and no way to read a name out of a value, so
// the listing has to exist for the type to exist at all. What is *not*
// repeated is any behaviour: [`verdicts`] is still the one place a matcher is
// decided, and the listing is a naming that a reader can check against it by
// eye.
//
// [`Matchers`] is generic in what a matcher *returns*, which is what lets
// `.resolves` reuse the one listing: the same forty-one names, each handing
// back a promise.
//
// # The received value's type is not carried, and that was tried
//
// ubugeeei-prod/uf#402 asked for a second parameter as well — the type handed
// to `expect`, carried through the matchers so that `expect(count).toBe("two")`
// is an error at the call. It is written here rather than left for somebody to
// discover, because it looks obviously right and is not.
//
// `toBe` is `Object.is`, and identity is not assignability. Typing it
// `(expected: T)` demands that the expected value be a subtype of the received
// one, which is a direction the runtime has no opinion about and which ordinary
// assertions fail in both:
// `expect(document.activeElement).toBe(screen.getByRole("button"))` compares an
// `HTMLElement | null` with an `Element`, neither is the other's subtype, and
// that is the most common assertion in a DOM test. Jest and Vitest both type
// this argument as `unknown` for the same reason.
//
// Worse, the parameter has to be *inferred*, and `expect(x)` is where a lot of
// otherwise unconstrained expressions sit. `await
// expect(client.request("/users/1")).resolves.toEqual({ id: 1 })` stops
// checking and starts reporting that `request`'s own type parameter is
// underconstrained, because a `mixed` parameter asked nothing of the argument
// and a generic one asks it to be solved. `expect([])` becomes "cannot
// determine type of empty array literal". Both are correct tests, and the type
// that rejects them is worse than the type that missed a mistyped comparison.
//
// So the received value arrives as `mixed`, and what makes a matcher checked is
// its own signature: `toHaveLength` wants a number, `toMatch` a pattern,
// `toBeTypeOf` one of the eight words `typeof` answers with, and every name is
// a name. The comparison between two unrelated types stays unchecked, and it is
// the only part of the issue that does.

import type { AsymmetricMatcher } from "./asymmetric.js";
import type { AxeOptions } from "./axe.js";
import type { SpyCall } from "./spy.js";
import * as asymmetric from "./asymmetric.js";
import * as snapshot from "./snapshot.js";
import { auditElement, describeViolations, violationIds } from "./axe.js";
import { isSpy } from "./spy.js";
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

/**
 * What a matcher decided, and how to say it either way.
 *
 * A matcher may answer with a promise of one. Only one does — the accessibility
 * audit, whose engine has no synchronous entry point — and [`bind`] is where
 * the two cases are told apart; see the note there for why the promise is not
 * hidden from the caller.
 */
type Verdict = {|
  readonly pass: boolean,
  readonly failure: () => string,
  readonly negatedFailure: () => string,
  readonly expected?: string,
  readonly received?: string,
|};

/** What `typeof` can answer, for the matcher that compares against it. */
type TypeName =
  | "bigint"
  | "boolean"
  | "function"
  | "number"
  | "object"
  | "string"
  | "symbol"
  | "undefined";

/**
 * Every matcher, each returning `R`.
 *
 * Generic in the return type because the surface exists twice and that is the
 * only thing that differs: `expect(x)` raises where a matcher fails and hands
 * back nothing, while `expect(p).resolves` settles first and so hands back a
 * promise. Writing the forty-one names once and saying what changes is the
 * whole reason for the parameter — the alternative was the same list twice,
 * with `=> void` on one copy and `=> Promise<void>` on the other, and a reader
 * left to diff them.
 *
 * Where an argument is `mixed` it is because the runtime genuinely takes
 * anything there and the checker would be lying to say otherwise: `toEqual`
 * accepts an asymmetric matcher standing in for a value at any depth, and
 * `toHaveValue` compares whatever a control is holding. Where it is not —
 * `toHaveLength` wants a number, `toMatch` a string or a pattern, `toBeTypeOf`
 * one of the eight words `typeof` produces — the narrower type is what the
 * implementation already assumes, and saying it out loud is the point of the
 * exercise.
 *
 * `not` is the same list again because negation is the only thing it changes.
 * `resolves` and `rejects` are deliberately not here: they belong to
 * [`Expectation`], because `expect(p).resolves.not` exists and
 * `expect(x).not.resolves` does not.
 */
export type Matchers<R> = {
  readonly toBe: (expected: mixed) => R,
  readonly toEqual: (expected: mixed) => R,
  readonly toStrictEqual: (expected: mixed) => R,
  readonly toBeTruthy: () => R,
  readonly toBeFalsy: () => R,
  readonly toBeNull: () => R,
  readonly toBeUndefined: () => R,
  readonly toBeDefined: () => R,
  readonly toBeNaN: () => R,
  readonly toBeGreaterThan: (expected: number) => R,
  readonly toBeGreaterThanOrEqual: (expected: number) => R,
  readonly toBeLessThan: (expected: number) => R,
  readonly toBeLessThanOrEqual: (expected: number) => R,
  readonly toBeCloseTo: (expected: number, digits?: number) => R,
  readonly toContain: (expected: mixed) => R,
  readonly toContainEqual: (expected: mixed) => R,
  readonly toHaveLength: (expected: number) => R,
  readonly toHaveProperty: (path: string, ...rest: $ReadOnlyArray<mixed>) => R,
  readonly toMatch: (expected: string | RegExp) => R,
  readonly toMatchObject: (expected: mixed) => R,
  readonly toBeInstanceOf: (expected: mixed) => R,
  readonly toBeTypeOf: (expected: TypeName) => R,
  readonly toSatisfy: (predicate: (value: mixed) => boolean) => R,
  readonly toMatchSnapshot: (hint?: string) => R,
  readonly toMatchInlineSnapshot: (expected?: string) => R,
  readonly toThrow: (...rest: $ReadOnlyArray<mixed>) => R,
  readonly toHaveBeenCalled: () => R,
  readonly toHaveBeenCalledTimes: (count: number) => R,
  readonly toHaveBeenCalledWith: (...args: $ReadOnlyArray<mixed>) => R,
  readonly toHaveBeenLastCalledWith: (...args: $ReadOnlyArray<mixed>) => R,
  readonly toBeInTheDocument: () => R,
  readonly toBeVisible: () => R,
  readonly toBeDisabled: () => R,
  readonly toBeEnabled: () => R,
  readonly toBeChecked: () => R,
  readonly toBeRequired: () => R,
  readonly toHaveFocus: () => R,
  readonly toHaveAttribute: (name: string, value?: mixed) => R,
  readonly toHaveClass: (...names: $ReadOnlyArray<string>) => R,
  readonly toHaveTextContent: (expected: string | RegExp) => R,
  readonly toHaveValue: (expected: mixed) => R,
  /**
   * Run axe-core over this element's subtree and require it to find nothing.
   *
   * `Promise<void>` rather than `R`, and it is the one matcher in this listing
   * that is not generic: axe has no synchronous entry point, so the answer is
   * a promise however the expectation was reached, and `await` is not optional.
   *
   *     await expect(container).toHaveNoAxeViolations();
   *     await expect(container).toHaveNoAxeViolations({ tags: ["wcag2a"] });
   *
   * The rule set comes from `accessibility.axe` in `uf.config.js`; the argument
   * narrows it for one assertion. See `./axe.js`.
   */
  readonly toHaveNoAxeViolations: (options?: AxeOptions) => Promise<void>,
  readonly not: Matchers<R>,
  ...
};

/**
 * What `expect(received)` hands back.
 *
 * The matchers, plus the two that settle a promise before applying them. An
 * intersection rather than a copy of the list with two lines added, and
 * rather than an object spread, because a spread of an object type drops the
 * `readonly` off every property it carries over — Flow computes a fresh object
 * from the spread and the fresh one is writable, which would publish forty-one
 * assignable matchers.
 *
 * # What is still not checked
 *
 * `expect(5).resolves` types, and fails when it runs. Saying otherwise needs
 * `expect` to have two call signatures — one for a promise handing back a
 * shape with `resolves`, one for everything else handing back a shape without
 * — and Flow then requires the single function behind them to satisfy both,
 * which no single function does. The overload is written down here rather than
 * attempted because "it did not type" is the kind of thing that gets tried
 * twice.
 */
export type Expectation = Matchers<void> & {
  readonly resolves: Matchers<Promise<void>>,
  readonly rejects: Matchers<Promise<void>>,
  ...
};

/**
 * `expect` itself: callable, and carrying the matchers that stand in for a
 * value instead of being one.
 *
 * Inexact, and it has to be. The value is a function, every function has
 * `name`, `length`, `call`, `apply` and `bind`, and an exact object type
 * refuses one for exactly that reason. Inexactness costs nothing that matters
 * here: Flow still reports a read of a property this type does not list, which
 * is what makes `expect.anythign()` an error.
 */
export type Expect = {
  (received: mixed): Expectation,
  // `flow/unclear-type` reads source text rather than an AST, and the shape it
  // recognises as a property key rather than a type is a name at the start of
  // a line or straight after `{`, `,` or `;`. `readonly any:` is neither, so
  // the rule reports Jest's, Vitest's and Sinon's name for this matcher as an
  // `any` type. The rule's own comment already lists `@uniflowed/test`'s
  // `expect.any` among the false positives it exists to avoid; this is the one
  // spelling it still cannot see past.
  // uf-lint-disable-next-line flow/unclear-type
  readonly any: (constructor: mixed) => AsymmetricMatcher,
  readonly anything: () => AsymmetricMatcher,
  readonly objectContaining: (expected: interface {}) => AsymmetricMatcher,
  readonly arrayContaining: (expected: $ReadOnlyArray<mixed>) => AsymmetricMatcher,
  readonly stringContaining: (substring: string) => AsymmetricMatcher,
  readonly stringMatching: (pattern: string | RegExp) => AsymmetricMatcher,
  readonly closeTo: (value: number, digits?: number) => AsymmetricMatcher,
  readonly not: {
    readonly objectContaining: (expected: interface {}) => AsymmetricMatcher,
    readonly arrayContaining: (expected: $ReadOnlyArray<mixed>) => AsymmetricMatcher,
    readonly stringContaining: (substring: string) => AsymmetricMatcher,
    readonly stringMatching: (pattern: string | RegExp) => AsymmetricMatcher,
    readonly closeTo: (value: number, digits?: number) => AsymmetricMatcher,
    ...
  },
  ...
};

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
 *
 * # Every entry takes `mixed`, and that is what makes the indexer sayable
 *
 * [`bind`] reaches an entry by a computed key and applies it to the
 * `$ReadOnlyArray<mixed>` it collected from the caller, so the indexer has to
 * describe a function that will accept those arguments. Parameters are
 * contravariant, so an entry that demanded a `number` could not be described
 * by one — `(...args: $ReadOnlyArray<mixed>)` rejects it, and
 * `(...args: $ReadOnlyArray<empty>)` accepts it and rejects the call. That
 * disagreement is why this indexer used to be written `$ReadOnlyArray<any>`,
 * with a `flow/unclear-type` suppression on it.
 *
 * So the disagreement is gone instead of papered over: every entry here takes
 * `mixed` and coerces what it needs, the way most of them — `toBe`,
 * `toBeGreaterThan`, `toHaveAttribute` — already did. Nothing a caller can see
 * got wider: `toHaveLength` still refuses a string and `toBeTypeOf` still
 * refuses a word `typeof` never says, because those are [`Matchers`]'s
 * signatures and [`Matchers`] is the published type. What changed is that the
 * table behind them stopped claiming a narrower argument than the one `bind`
 * can hand it, which is a claim that was never true. The `String(…)` and
 * `Number(…)` calls that appeared with it are the coercion the runtime was
 * already doing, said out loud, and each produces the same message the implicit
 * one did for the same input.
 *
 * The alternative — narrowing the indexer by writing `bind`'s forty-one
 * wrappers out to avoid the computed lookup — is a second copy of the listing
 * to keep in step with [`Matchers`], and is a worse trade than either. See
 * ubugeeei-prod/uf#402.
 */
function verdicts(received: mixed): {
  readonly [string]: (...args: $ReadOnlyArray<mixed>) => Verdict | Promise<Verdict>,
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
    toBeCloseTo: (expected: mixed, digits?: mixed) => {
      const target = Number(expected);
      const places = digits === undefined ? 2 : Number(digits);
      const tolerance = 10 ** -places / 2;
      const difference = Math.abs((received as $FlowFixMe) - target);
      return simple(
        difference < tolerance,
        `to be within ${tolerance} of ${target}, but it is off by ${difference}`,
        target,
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
    toHaveLength: (expected: mixed) => {
      const length = received == null ? undefined : (received as $FlowFixMe).length;
      return simple(
        length === expected,
        `to have length ${String(expected)}, not ${render(length)}`,
        expected,
      );
    },
    toHaveProperty: (path: mixed, ...rest: $ReadOnlyArray<mixed>) => {
      const at = String(path);
      const found = propertyAt(received, at);
      if (rest.length === 0) {
        return simple(found.found, `to have a property at \`${at}\``);
      }
      return simple(
        found.found && equals(found.value, rest[0]),
        `to have \`${at}\` equal to ${render(rest[0])}, not ${render(found.value)}`,
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
    toBeTypeOf: (expected: mixed) => {
      const name = String(expected);
      return simple(
        typeof received === name,
        `to be of type ${name}, not ${typeof received}`,
        name,
      );
    },
    toSatisfy: (predicate: mixed) =>
      simple(
        typeof predicate === "function" && predicate(received) === true,
        "to satisfy the predicate",
      ),
    toMatchSnapshot: (hint?: mixed): Verdict => {
      const verdict = snapshot.matchSnapshot(
        received,
        hint === undefined ? undefined : String(hint),
      );
      return {
        pass: verdict.pass,
        expected: verdict.expected ?? "(no snapshot yet)",
        received: verdict.received,
        // The whole of both sides, because a mismatch that says only "the
        // snapshot did not match" makes a reader open two files.
        failure: () =>
          `snapshot did not match.\n\nstored:\n${verdict.expected ?? "(none)"}\n\n` +
          `received:\n${verdict.received}\n\n` +
          "Run `uf test -u` if the new value is the right one.",
        negatedFailure: () => "expected the value not to match its snapshot",
      };
    },
    toMatchInlineSnapshot: (expected?: mixed): Verdict => {
      const verdict = snapshot.matchInlineSnapshot(
        received,
        expected === undefined ? undefined : String(expected),
      );
      return {
        pass: verdict.pass,
        expected: verdict.expected ?? "(no inline snapshot yet)",
        received: verdict.received,
        failure: () =>
          verdict.expected == null
            ? // uf does not rewrite a test file — a tool that edits the file you
              // are editing is a tool that loses work — so it reports what to
              // paste in and leaves the decision to a person.
              `no inline snapshot yet. Paste this into the call:\n\n\`\`\`\n${verdict.received}\n\`\`\``
            : `inline snapshot did not match.\n\nstored:\n${verdict.expected}\n\n` +
              `received:\n${verdict.received}`,
        negatedFailure: () => "expected the value not to match its inline snapshot",
      };
    },
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
    toHaveBeenCalledTimes: (count: mixed) => {
      requireSpy("toHaveBeenCalledTimes");
      const actual = spyCalls().length;
      return simple(
        actual === count,
        `to have been called ${String(count)} times, not ${actual}`,
        count,
      );
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
    toHaveNoAxeViolations: async (options: mixed) => {
      const node = element("toHaveNoAxeViolations");
      const found = await auditElement(node, (options: $FlowFixMe));
      const named = violationIds(found);
      return {
        pass: found.length === 0,
        expected: "no accessibility violations",
        received: found.length === 0 ? "none" : named,
        failure: () =>
          `expected no accessibility violations, and axe-core reported ` +
          `${String(found.length)}:\n${describeViolations(found)}`,
        // A passing audit is not proof of an accessible component — axe finds
        // what a machine can find — so the negated message says what was
        // actually established rather than implying the opposite verdict.
        negatedFailure: () => "expected axe-core to report a violation, and it reported none",
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
      // All four arguments, unlike the first version of this: the runner
      // renders `expected` and `received` beside the message, and a one
      // argument call left both of them `undefined` on screen for the one
      // failure whose whole content is what was received instead.
      throw new AssertionError(
        `${matcher} needs an element, and received ${render(received)}`,
        matcher,
        "an element",
        render(received),
      );
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
 *
 * # The one thing a closed `<details>` still shows
 *
 * Its `<summary>`. A closed disclosure renders exactly one child and hides the
 * rest, so the rule is "everything under a closed `<details>` except its
 * summary" — and saying that needs the child the walk arrived from, not only
 * the ancestor it is standing on. Without it the rule was written as "unless
 * the `<details>` is the element being asked about", which exempted the
 * disclosure from its own rule and left the summary inside it invisible:
 * `expect(screen.getByText("More")).toBeVisible()` failed for the one thing on
 * the screen, while the reader was looking at it.
 *
 * # Why this is not the walk in `react-testing`
 *
 * `packages/react-testing/internal/queries.js` has one that looks like this
 * and answers a different question. `exposed` asks whether the accessibility
 * tree announces the element, so it ignores `opacity: 0` — a screen reader
 * reads text at zero opacity, which is exactly why hiding text that way is a
 * bug rather than a technique — and it takes `aria-hidden` as decisive. This
 * one asks whether a reader would *see* it, so the two answers part company
 * there on purpose. The `<details>` half is the half they agree on, and it is
 * written the same way in both.
 */
function isVisible(node: Element): boolean {
  let child: $FlowFixMe = null;
  let current: $FlowFixMe = node;
  while (current != null && current.nodeType === 1) {
    if (current.hasAttribute("hidden") || current.getAttribute("aria-hidden") === "true") {
      return false;
    }
    if (
      child != null &&
      current.tagName === "DETAILS" &&
      !current.hasAttribute("open") &&
      child.tagName !== "SUMMARY"
    ) {
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
    child = current;
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
 *
 * # Why the object is built rather than written
 *
 * `.not` has to be reached lazily or building an expectation would build its
 * negation, which would build *its* negation, forever. A lazily installed
 * property is not something an object literal carries, so the value is
 * completed with `Object.defineProperty` after it exists — and an object
 * completed after the fact is not one Flow can check a literal against. That
 * is what this `$FlowFixMe` is, and it now covers a construction rather than a
 * published type: [`expectValue`] states the real one, and the checker holds
 * every caller to it.
 */
function bind(received: mixed, negated: boolean): $FlowFixMe {
  const table = verdicts(received);
  const bound: $FlowFixMe = {};
  for (const name of Object.keys(table)) {
    const decide = (verdict: Verdict) => {
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
    bound[name] = (...args: $ReadOnlyArray<mixed>) => {
      const verdict = table[name](...args);
      // A matcher whose engine is asynchronous answers with a promise of a
      // verdict, and the promise is handed straight back rather than hidden.
      //
      // Hiding it was the alternative and it cannot be done: the only way to
      // present an asynchronous answer synchronously is to decide before it
      // arrives, which is deciding without it. What the promise costs is a
      // forgotten `await`, and that case is not silent either — the rejection
      // reaches the worker's unhandled-rejection handler, which fails the file
      // the promise was created in and prints this same message. A missing
      // `await` on a passing audit is the one case nothing reports, and it is
      // the case where nothing happened.
      //
      // `instanceof Promise` rather than a `then` test, and it is safe for a
      // reason that would not survive being generalised: every entry in the
      // table is written in this file, so the only promise that can arrive
      // here is one an `async` function in this module made, in this realm. A
      // matcher registered from outside — which `@uniflowed/test` has no API
      // for, deliberately — could hand back a foreign thenable, and this line
      // would be the thing to revisit.
      return verdict instanceof Promise ? verdict.then(decide) : decide(verdict);
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
      // Returned rather than called and dropped: the wrapper is `async`, so
      // returning an asynchronous matcher's promise is what awaits it. Without
      // this, `await expect(p).resolves.toHaveNoAxeViolations()` awaited the
      // settling and not the audit, and a violation surfaced as an unhandled
      // rejection under whichever file was running by then.
      return bind(subject, negated)[name](...args);
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
 *
 * Takes a `mixed` and hands back a written-out [`Expectation`]. What each
 * matcher will accept is decided by its own signature rather than by what was
 * received, for the reasons this module's header sets out.
 */
function expectValue(received: mixed): Expectation {
  const expectation: $FlowFixMe = bind(received, false);
  Object.defineProperty(expectation, "resolves", {
    get: () => settled(received, "resolve", false),
  });
  Object.defineProperty(expectation, "rejects", { get: () => settled(received, "reject", false) });
  return expectation;
}

/**
 * Build the callable that carries the asymmetric matchers.
 *
 * The statics are attached inside a builder rather than at the module's top
 * level, the way `internal/registry.js` builds `describe`: a shipped module
 * may only declare, import and export at its top level, and `expect.any = …`
 * out here is a statement that runs when the module is imported.
 *
 * It was `Object.assign(expectValue, { … })`, which is what a reader expects
 * and what does not type. Flow models `Object.assign` as returning the
 * *target*, so the result of assigning matchers onto a function is still a
 * function with no matchers on it — eight `prop-missing` errors saying so, and
 * a `flow/unsafe-object-assign` suppression on top of them. Attaching to a
 * local before it is returned is the same runtime value with none of that: the
 * checker sees the statics arrive and holds the result to [`Expect`].
 */
function expecting(): Expect {
  const api = (received: mixed): Expectation => expectValue(received);
  api.any = asymmetric.any;
  api.anything = asymmetric.anything;
  api.objectContaining = asymmetric.objectContaining;
  api.arrayContaining = asymmetric.arrayContaining;
  api.stringContaining = asymmetric.stringContaining;
  api.stringMatching = asymmetric.stringMatching;
  api.closeTo = asymmetric.closeTo;
  api.not = {
    objectContaining: (expected: interface {}) =>
      asymmetric.not(asymmetric.objectContaining(expected)),
    arrayContaining: (expected: $ReadOnlyArray<mixed>) =>
      asymmetric.not(asymmetric.arrayContaining(expected)),
    stringContaining: (substring: string) => asymmetric.not(asymmetric.stringContaining(substring)),
    stringMatching: (pattern: string | RegExp) =>
      asymmetric.not(asymmetric.stringMatching(pattern)),
    closeTo: (value: number, digits?: number) => asymmetric.not(asymmetric.closeTo(value, digits)),
  };
  return api;
}

/**
 * Assert about a value.
 *
 * The `expect.*` half are the matchers that stand in for a value instead of
 * being one. `expect(user).toEqual({ id: expect.any(String), name: "uf" })`
 * says what a test means; spelling out the id would either be a lie or a second
 * source of truth. They work at any depth, because `equals` asks every value it
 * meets whether it is one.
 *
 * `expect.not.*` is the negated form, spelled the way Jest and Vitest spell it
 * — `expect.not.objectContaining({ error: expect.anything() })` reads better
 * than a negated assertion around the whole object, and is the form a suite
 * being ported will already have.
 */
export const expect: Expect = expecting();
