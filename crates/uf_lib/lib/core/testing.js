// @flow
//
// `@uniflowed/testing`. Owns the base test API; the React Testing Library
// surface is re-exported by name from `./react-testing.js`.

import { nativeRuntimeRequired } from "./internal/native-runtime.js";

const MODULE = "@uniflowed/core/testing";

export type TestBody = () => mixed | Promise<mixed>;

export interface Expectation<T> {
  toBe(expected: T): void,
  toEqual(expected: mixed): void,
  toBeVisible(): void,
  resolves: Expectation<T>,
  rejects: Expectation<T>,
}

export function describe(name: string, body: TestBody): void {
  return nativeRuntimeRequired(MODULE, "describe");
}

export function it(name: string, body: TestBody): void {
  return nativeRuntimeRequired(MODULE, "it");
}

export function test(name: string, body: TestBody): void {
  return nativeRuntimeRequired(MODULE, "test");
}

export function beforeEach(body: TestBody): void {
  return nativeRuntimeRequired(MODULE, "beforeEach");
}

export function afterEach(body: TestBody): void {
  return nativeRuntimeRequired(MODULE, "afterEach");
}

export function expect<T>(value: T): Expectation<T> {
  return nativeRuntimeRequired(MODULE, "expect");
}

export type { RenderResult, Screen } from "./react-testing.js";
export {
  fireEvent,
  render,
  screen,
  userEvent,
  waitFor,
} from "./react-testing.js";
