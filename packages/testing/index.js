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
// `@uniflowed/react-testing`, which installs a document on the host the first
// time a test renders.

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
export {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  userEvent,
  waitFor,
  within,
} from "@uniflowed/react-testing";
