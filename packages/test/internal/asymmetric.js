// @flow
//
// Internal to `@uniflowed/test`: `expect.any`, `expect.objectContaining`, and
// the rest of the matchers that stand in for a value instead of being one.
//
// They exist because most assertions are about the parts of a value a test
// controls, and equality is about all of it. `expect(user).toEqual({ id:
// expect.any(String), name: "uf" })` says what the test means; spelling out the
// id would either be a lie or a second source of truth.
//
// The mechanism is one method. A matcher is an object carrying
// `asymmetricMatch(received)`, and `equals` asks any value it meets whether it
// is one — so a matcher nested six levels down inside an expected object works
// for the same reason a top-level one does, with no special case anywhere.
// Vitest and Jest use the same protocol, which is why a matcher from either
// works here.

// `equality` and this module are mutually recursive, and that is the domain:
// comparing two values has to recognise a matcher, and a matcher has to compare
// recursively. ESM handles a cycle between hoisted function declarations — by
// the time any matcher runs, both modules have finished evaluating — and the
// alternative was wiring the comparison in at load, which is an import-time side
// effect and the one thing every shipped module here is forbidden.
import { equals } from "./equality.js";

/** The brand every matcher carries, so `equals` can recognise one. */
const ASYMMETRIC = "$$uf.asymmetricMatch";

/** A stand-in for a value, rather than a value. */
export type AsymmetricMatcher = {
  readonly [typeof ASYMMETRIC]: true,
  readonly asymmetricMatch: (received: mixed) => boolean,
  readonly toString: () => string,
};

/**
 * Whether `value` is a matcher rather than something to compare against.
 *
 * Duck-typed on `asymmetricMatch` as well as uf's own brand, so a matcher built
 * by Jest or Vitest — or by a project's own helper — is recognised too. The
 * protocol is the interface; the brand is only a fast path.
 */
export function isAsymmetric(value: mixed): boolean {
  if (value == null || typeof value !== "object") {
    return false;
  }
  const candidate = value as $FlowFixMe;
  return candidate[ASYMMETRIC] === true || typeof candidate.asymmetricMatch === "function";
}

/** Ask a matcher whether `received` satisfies it. */
export function matchesAsymmetric(matcher: mixed, received: mixed): boolean {
  return (matcher as $FlowFixMe).asymmetricMatch(received) === true;
}

/** Build a matcher from a predicate and how it describes itself. */
function matcher(label: string, predicate: (received: mixed) => boolean): AsymmetricMatcher {
  return {
    [ASYMMETRIC]: true,
    asymmetricMatch: predicate,
    toString: () => label,
  };
}

/**
 * Anything constructed by `constructor`, or any value of a primitive's type.
 *
 * `expect.any(String)` accepts `"uf"` as well as `new String("uf")`, because a
 * string literal is not an instance of anything and a test that wrote
 * `expect.any(String)` meant the type rather than the wrapper. The same for
 * `Number`, `Boolean`, `BigInt`, `Symbol` and `Function`.
 */
export function any(constructor: mixed): AsymmetricMatcher {
  const name = (constructor as $FlowFixMe)?.name ?? String(constructor);
  return matcher(`Any<${name}>`, (received) => {
    switch (constructor) {
      case String:
        return typeof received === "string" || received instanceof String;
      case Number:
        return typeof received === "number" || received instanceof Number;
      case Boolean:
        return typeof received === "boolean" || received instanceof Boolean;
      case BigInt:
        return typeof received === "bigint";
      case Symbol:
        return typeof received === "symbol";
      case Function:
        return typeof received === "function";
      case Object:
        // `Object` means "any non-null object", which is what a test asking for
        // one means — not "has Object.prototype in its chain", which a
        // null-prototype object would fail and a test would find baffling.
        return received != null && (typeof received === "object" || typeof received === "function");
      default:
        return typeof constructor === "function" && received instanceof constructor;
    }
  });
}

/** Anything at all except `null` and `undefined`. */
export function anything(): AsymmetricMatcher {
  return matcher("Anything", (received) => received != null);
}

/**
 * An object with at least these properties, compared with `equals`.
 *
 * The comparison is recursive, so a matcher nested inside `expected` works.
 */
export function objectContaining(expected: interface {}): AsymmetricMatcher {
  return matcher(`ObjectContaining(${describe(expected)})`, (received) => {
    if (received == null || typeof received !== "object") {
      return false;
    }
    const target = received as $FlowFixMe;
    const source = expected as $FlowFixMe;
    for (const key of Object.keys(source)) {
      if (!(key in target) || !equals(target[key], source[key])) {
        return false;
      }
    }
    return true;
  });
}

/** An array holding at least these elements, in any order. */
export function arrayContaining(expected: $ReadOnlyArray<mixed>): AsymmetricMatcher {
  return matcher(`ArrayContaining(${describe(expected)})`, (received) => {
    if (!Array.isArray(received)) {
      return false;
    }
    return expected.every((wanted) => received.some((item) => equals(item, wanted)));
  });
}

/** A string containing `substring`. */
export function stringContaining(substring: string): AsymmetricMatcher {
  return matcher(`StringContaining(${JSON.stringify(substring)})`, (received) => {
    return typeof received === "string" && received.includes(substring);
  });
}

/** A string the pattern matches. */
export function stringMatching(pattern: string | RegExp): AsymmetricMatcher {
  return matcher(`StringMatching(${String(pattern)})`, (received) => {
    if (typeof received !== "string") {
      return false;
    }
    return typeof pattern === "string" ? received.includes(pattern) : pattern.test(received);
  });
}

/** Default digits of precision for `closeTo`, matching Jest and Vitest. */
const CLOSE_TO_DIGITS = 2;

/**
 * A number within `10 ** -digits / 2` of `expected`.
 *
 * The same tolerance `toBeCloseTo` uses, so the asymmetric form and the matcher
 * agree — a test that moves an assertion from one to the other should not have
 * its verdict change.
 */
export function closeTo(expected: number, digits: number = CLOSE_TO_DIGITS): AsymmetricMatcher {
  const tolerance = 10 ** -digits / 2;
  return matcher(`CloseTo(${expected}, ${digits})`, (received) => {
    return typeof received === "number" && Math.abs(received - expected) < tolerance;
  });
}

/** A matcher that holds when `inner` does not. */
export function not(inner: AsymmetricMatcher): AsymmetricMatcher {
  return matcher(`Not(${inner.toString()})`, (received) => !matchesAsymmetric(inner, received));
}

/** A short rendering of an expected value, for a matcher's own name. */
function describe(value: mixed): string {
  try {
    return JSON.stringify(value) ?? String(value);
  } catch {
    // A cyclic or otherwise unserialisable expectation still has to have a
    // name; what it is called matters less than that naming it cannot throw.
    return String(value);
  }
}
