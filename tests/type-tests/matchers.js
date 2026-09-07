// @flow
//
// Assertions that have to be type errors, and the test that says they are.
//
// This file is *supposed* to fail `uf check`. Every line below is a mistake
// somebody makes while writing a test, and until `@uniflowed/test`'s `expect`
// had a type not one of them was reported by anything: `expect` was
// `$FlowFixMe` and so was everything it handed back, so a misspelt matcher, an
// argument of the wrong type and a comparison between two different types were
// all fine until the suite ran — and a matcher that does not exist does not
// fail, it does nothing.
//
// # How it is read
//
// A `// expect:` comment says that the line after it must be reported, and that
// the report must contain that text. A line without one must not be reported at
// all — so a change that makes any of these *stop* being an error fails the
// test, and so does one that makes something else here start being one.
//
// The unmarked half at the bottom is doing as much work as the marked half.
// Four of those lines are correct assertions that a stricter `expect` — one
// carrying the received value's type through its matchers — refuses, which is
// the one thing ubugeeei-prod/uf#402 asked for that is deliberately not here.
// `packages/test/internal/expect.js` says why at length; this is where trying
// it again fails.
//
// One thing the issue asked for is missing and not marked: `expect(5).resolves`
// still types and still fails when it runs. It would take two call signatures
// on `expect`, and Flow requires the one function behind them to satisfy both.
//
// # Why it is checked with the package rather than on its own
//
// `uf check` builds its module map out of the files it is asked to check, and a
// relative import that leaves that set resolves to an any-typed value — after
// which `expect` is `any` again and every line below passes, which is the exact
// state this file exists to prevent returning to. So the test runs
// `uf check tests/type-tests packages/test packages/react-testing`, with all of
// them in one set. `anchoring.js`'s header says why the fixtures live here
// rather than inside the packages they are about.

import { expect } from "../../packages/test/index.js";

declare var list: Array<string>;
declare var count: number;
declare var promised: Promise<Array<string>>;
declare var active: HTMLElement | null;
declare var found: Element;
declare function requested<TBody>(): Promise<TBody>;

// A matcher that does not exist. This is the one that stings: an assertion
// nobody implements throws nothing, passes, and reads in review as a test.
// expect: toBaa
export const misspeltMatcher: void = expect(list).toBaa(1);

// The same, underneath `.not`, which is the same table.
// expect: toBaa
export const misspeltUnderNot: void = expect(list).not.toBaa(1);

// A length is a number.
// expect: number
export const lengthIsANumber: void = expect(list).toHaveLength("3");

// And so is a count of calls, and a tolerance.
// expect: number
export const timesIsANumber: void = expect(list).toHaveBeenCalledTimes("2");

// expect: number
export const closeToIsANumber: void = expect(count).toBeCloseTo("1");

// `toBeTypeOf` takes one of the eight words `typeof` answers with.
// expect: TypeName
export const typeOfTakesATypeName: void = expect(count).toBeTypeOf("strnig");

// A pattern is a string or a regular expression.
// expect: RegExp
export const matchTakesAPattern: void = expect(list).toMatch(3);

// A class name is a string.
// expect: string
export const classNamesAreStrings: void = expect(list).toHaveClass(1);

// The asymmetric matchers hanging off `expect` are named too.
// expect: anythign
export const misspeltAsymmetric: mixed = expect.anythign();

// And the settled halves apply the same table, so the same argument is wrong
// on the far side of an await.
// expect: number
export const throughResolves: Promise<void> = expect(promised).resolves.toHaveLength("3");

// Everything below is correct, and has to stay unreported: this half is what
// says the fixture is checking a real type rather than a broken one.
export const equalsAValue: void = expect(count).toBe(2);
export const negated: void = expect(count).not.toBe(3);
export const length: void = expect(list).toHaveLength(2);
export const closeTo: void = expect(count).toBeCloseTo(1, 3);
export const typed: void = expect(count).toBeTypeOf("number");
export const matched: void = expect(list).toMatch(/uf/);
export const settles: Promise<void> = expect(promised).resolves.toHaveLength(3);
export const rejects: Promise<void> = expect(promised).rejects.not.toBe(undefined);
export const asymmetric: void = expect(list).toEqual([expect.any(String)]);
export const negatedAsymmetric: void = expect(list).toEqual(
  expect.not.arrayContaining(["nothing"]),
);

// And these four are the reason `expect` takes a `mixed` rather than carrying
// the received value's type through the matchers, which is the one thing
// ubugeeei-prod/uf#402 asked for that is not here. Each of them is a correct
// assertion that a generic `expect` refuses, and `internal/expect.js` says why
// at length. They are here so that trying it again fails at this file rather
// than in review.
export const identityAcrossTypes: void = expect(active).toBe(found);
export const identityTheOtherWay: void = expect(found).toBe(active);
export const anEmptyArray: void = expect([]).toEqual([]);
export const anUnconstrainedCall: Promise<void> = expect(requested()).resolves.toEqual({ id: 1 });
