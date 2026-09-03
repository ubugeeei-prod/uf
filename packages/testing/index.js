// @flow
//
// `@uniflowed/testing`: the test API under its other name.
//
// There is one implementation, in `@uniflowed/test`. This package exists
// because both names are documented and a project should not have to know
// which one the runner happens to be called; every binding is re-exported by
// name so a bundler still drops what an application does not use.
//
// The React Testing Library surface is re-exported from
// `@uniflowed/react-testing`, which still needs a DOM and says so when called.

export type { Body as TestBody, TestOptions } from "@uniflowed/test";

export {
  AssertionError,
  afterAll,
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  fn,
  it,
  test,
} from "@uniflowed/test";

export type { RenderResult, Screen } from "@uniflowed/react-testing";
export { fireEvent, render, screen, userEvent, waitFor } from "@uniflowed/react-testing";
