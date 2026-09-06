// @flow
//
// The six forms of every query, generated once.
//
// Writing `getByText`, `queryByText`, `findByText`, `getAllByText`,
// `queryAllByText` and `findAllByText` by hand, for six queries, is thirty-six
// functions that differ in two decisions: whether finding nothing is an error,
// and whether to wait. So the two decisions are written once and the thirty-six
// are derived — which also means a new query is one entry rather than six
// functions.
//
// # The names are written down, and the behaviour is not
//
// `Queries` used to be `{ readonly [string]: (matcher: mixed, options?: mixed)
// => any }`, which is a way of writing "this object has whatever you ask it
// for, and it is whatever you like". That is a hole in the published type of a
// package whose entire purpose is testing *typed* components:
// `screen.getByRole("button").valeu` was not a mistake anybody's checker would
// find, `screen.getByTest("save")` was not a misspelling, and
// `await screen.getByText("Save")` — the missing `find`, which is the single
// most common mistake this library invites — was fine.
//
// So the thirty-six names are written out below. Flow has no template literal
// types, so `getBy${Name}` is not something a type can compute; the names have
// to be listed for the type to exist at all. What is *not* repeated is any
// behaviour: `forms` is still the one place the six decisions are made, and the
// listing below is a naming, six lines per query, which is the part a reader
// wants to be able to check against the runtime by eye.

import {
  MATCHER_OPTION_KEYS,
  ROLE_OPTION_KEYS,
  allByDisplayValue,
  allByLabelText,
  allByPlaceholderText,
  allByRole,
  allByTestId,
  allByText,
  atCallSite,
  queryFailure,
  rejectUnknownOptions,
} from "./queries.js";
import type { Matcher, MatcherOptions, RoleOptions } from "./queries.js";
import { bodyOf } from "./dom.js";
import { waitFor } from "./render.js";

/**
 * One query's six forms, over whatever that query matches on.
 *
 * Generic in the target because `ByRole` does not take a `Matcher` — it takes
 * a role, which is a string and only a string, and a regular expression over
 * role names is a query that would silently match nothing. Generic in the
 * options because `ByRole` is also the only query with more than `exact` to
 * say.
 */
type Forms<TTarget, TOptions> = {|
  readonly get: (target: TTarget, options?: TOptions) => Element,
  readonly getAll: (target: TTarget, options?: TOptions) => Array<Element>,
  readonly query: (target: TTarget, options?: TOptions) => Element | null,
  readonly queryAll: (target: TTarget, options?: TOptions) => Array<Element>,
  readonly find: (target: TTarget, options?: TOptions) => Promise<Element>,
  readonly findAll: (target: TTarget, options?: TOptions) => Promise<Array<Element>>,
|};

/**
 * The queries available on `screen` and on `within(element)`.
 *
 * Read down one column and the four questions of the module comment are the
 * four return types: `getBy…` is an `Element` because it throws rather than
 * hand back nothing, `queryBy…` is `Element | null` because its whole purpose
 * is asking about absence, and the `findBy…` pair are promises because they
 * wait.
 */
export type Queries = {|
  readonly getByText: (matcher: Matcher, options?: MatcherOptions) => Element,
  readonly getAllByText: (matcher: Matcher, options?: MatcherOptions) => Array<Element>,
  readonly queryByText: (matcher: Matcher, options?: MatcherOptions) => Element | null,
  readonly queryAllByText: (matcher: Matcher, options?: MatcherOptions) => Array<Element>,
  readonly findByText: (matcher: Matcher, options?: MatcherOptions) => Promise<Element>,
  readonly findAllByText: (matcher: Matcher, options?: MatcherOptions) => Promise<Array<Element>>,

  readonly getByRole: (role: string, options?: RoleOptions) => Element,
  readonly getAllByRole: (role: string, options?: RoleOptions) => Array<Element>,
  readonly queryByRole: (role: string, options?: RoleOptions) => Element | null,
  readonly queryAllByRole: (role: string, options?: RoleOptions) => Array<Element>,
  readonly findByRole: (role: string, options?: RoleOptions) => Promise<Element>,
  readonly findAllByRole: (role: string, options?: RoleOptions) => Promise<Array<Element>>,

  readonly getByLabelText: (matcher: Matcher, options?: MatcherOptions) => Element,
  readonly getAllByLabelText: (matcher: Matcher, options?: MatcherOptions) => Array<Element>,
  readonly queryByLabelText: (matcher: Matcher, options?: MatcherOptions) => Element | null,
  readonly queryAllByLabelText: (matcher: Matcher, options?: MatcherOptions) => Array<Element>,
  readonly findByLabelText: (matcher: Matcher, options?: MatcherOptions) => Promise<Element>,
  readonly findAllByLabelText: (
    matcher: Matcher,
    options?: MatcherOptions,
  ) => Promise<Array<Element>>,

  readonly getByPlaceholderText: (matcher: Matcher, options?: MatcherOptions) => Element,
  readonly getAllByPlaceholderText: (matcher: Matcher, options?: MatcherOptions) => Array<Element>,
  readonly queryByPlaceholderText: (matcher: Matcher, options?: MatcherOptions) => Element | null,
  readonly queryAllByPlaceholderText: (
    matcher: Matcher,
    options?: MatcherOptions,
  ) => Array<Element>,
  readonly findByPlaceholderText: (matcher: Matcher, options?: MatcherOptions) => Promise<Element>,
  readonly findAllByPlaceholderText: (
    matcher: Matcher,
    options?: MatcherOptions,
  ) => Promise<Array<Element>>,

  readonly getByTestId: (matcher: Matcher, options?: MatcherOptions) => Element,
  readonly getAllByTestId: (matcher: Matcher, options?: MatcherOptions) => Array<Element>,
  readonly queryByTestId: (matcher: Matcher, options?: MatcherOptions) => Element | null,
  readonly queryAllByTestId: (matcher: Matcher, options?: MatcherOptions) => Array<Element>,
  readonly findByTestId: (matcher: Matcher, options?: MatcherOptions) => Promise<Element>,
  readonly findAllByTestId: (matcher: Matcher, options?: MatcherOptions) => Promise<Array<Element>>,

  readonly getByDisplayValue: (matcher: Matcher, options?: MatcherOptions) => Element,
  readonly getAllByDisplayValue: (matcher: Matcher, options?: MatcherOptions) => Array<Element>,
  readonly queryByDisplayValue: (matcher: Matcher, options?: MatcherOptions) => Element | null,
  readonly queryAllByDisplayValue: (matcher: Matcher, options?: MatcherOptions) => Array<Element>,
  readonly findByDisplayValue: (matcher: Matcher, options?: MatcherOptions) => Promise<Element>,
  readonly findAllByDisplayValue: (
    matcher: Matcher,
    options?: MatcherOptions,
  ) => Promise<Array<Element>>,
|};

/**
 * The six forms of one finder, bound to a root.
 *
 * `getBy` fails when there is not exactly one, and says how many it saw and
 * what the markup looked like, because "found 3 elements" and "found nothing"
 * are different bugs and a test that reports neither wastes the reader's time.
 *
 * `TTarget` is bounded by `Matcher` rather than left free because
 * `queryFailure` has to describe what was asked for, and it describes the
 * three things a matcher can be. A role is a string, so the bound holds and
 * the failure message is the same one it always was.
 *
 * `known` is the keys the query's options may have, and each of the six checks
 * before it looks at anything. Six lines rather than one inside `all`, because
 * the message has to name the function the reader typed — `getByRole`, not
 * "a role query" — and because the two waiting forms have to raise now rather
 * than a second from now: an option a query does not take is a mistake in the
 * test, not a condition that is about to come true.
 */
function forms<TTarget extends Matcher, TOptions>(
  name: string,
  find: (root: Element, target: TTarget, options?: TOptions) => Array<Element>,
  root: () => Element,
  known: $ReadOnlyArray<string>,
): Forms<TTarget, TOptions> {
  const all = (target: TTarget, options?: TOptions) => find(root(), target, options);
  const check = (form: string, options?: TOptions) => {
    rejectUnknownOptions(`${form}By${name}`, options, known);
  };

  return {
    getAll: (target, options) => {
      check("getAll", options);
      const found = all(target, options);
      if (found.length === 0) {
        throw queryFailure(`getAllBy${name}`, target, root(), 0);
      }
      return found;
    },
    queryAll: (target, options) => {
      check("queryAll", options);
      return all(target, options);
    },
    get: (target, options) => {
      check("get", options);
      const found = all(target, options);
      if (found.length !== 1) {
        throw queryFailure(`getBy${name}`, target, root(), found.length);
      }
      return found[0];
    },
    query: (target, options) => {
      check("query", options);
      const found = all(target, options);
      if (found.length > 1) {
        throw queryFailure(`queryBy${name}`, target, root(), found.length);
      }
      return found[0] ?? null;
    },
    // The two waiting forms build an error at the call and hand it to the
    // failure on the way out. A wait keeps the *last* attempt's failure, and
    // the last attempt runs from a timer: by then the stack under it is the
    // poll loop and nothing else, so the failure has no line of the test left
    // in it to report. The synchronous four need none of this — they throw
    // while the caller is still on the stack.
    find: async (target, options) => {
      check("find", options);
      const asked = new Error("asked here");
      try {
        return await waitFor(() => {
          const found = all(target, options);
          if (found.length !== 1) {
            throw queryFailure(`findBy${name}`, target, root(), found.length);
          }
          return found[0];
        });
      } catch (error) {
        throw atCallSite(error, asked);
      }
    },
    findAll: async (target, options) => {
      check("findAll", options);
      const asked = new Error("asked here");
      try {
        return await waitFor(() => {
          const found = all(target, options);
          if (found.length === 0) {
            throw queryFailure(`findAllBy${name}`, target, root(), 0);
          }
          return found;
        });
      } catch (error) {
        throw atCallSite(error, asked);
      }
    },
  };
}

function queriesFor(root: () => Element): Queries {
  const text = forms("Text", allByText, root, MATCHER_OPTION_KEYS);
  const role = forms("Role", allByRole, root, ROLE_OPTION_KEYS);
  const labelText = forms("LabelText", allByLabelText, root, MATCHER_OPTION_KEYS);
  const placeholderText = forms("PlaceholderText", allByPlaceholderText, root, MATCHER_OPTION_KEYS);
  const testId = forms("TestId", allByTestId, root, MATCHER_OPTION_KEYS);
  const displayValue = forms("DisplayValue", allByDisplayValue, root, MATCHER_OPTION_KEYS);

  return {
    getByText: text.get,
    getAllByText: text.getAll,
    queryByText: text.query,
    queryAllByText: text.queryAll,
    findByText: text.find,
    findAllByText: text.findAll,

    getByRole: role.get,
    getAllByRole: role.getAll,
    queryByRole: role.query,
    queryAllByRole: role.queryAll,
    findByRole: role.find,
    findAllByRole: role.findAll,

    getByLabelText: labelText.get,
    getAllByLabelText: labelText.getAll,
    queryByLabelText: labelText.query,
    queryAllByLabelText: labelText.queryAll,
    findByLabelText: labelText.find,
    findAllByLabelText: labelText.findAll,

    getByPlaceholderText: placeholderText.get,
    getAllByPlaceholderText: placeholderText.getAll,
    queryByPlaceholderText: placeholderText.query,
    queryAllByPlaceholderText: placeholderText.queryAll,
    findByPlaceholderText: placeholderText.find,
    findAllByPlaceholderText: placeholderText.findAll,

    getByTestId: testId.get,
    getAllByTestId: testId.getAll,
    queryByTestId: testId.query,
    queryAllByTestId: testId.queryAll,
    findByTestId: testId.find,
    findAllByTestId: testId.findAll,

    getByDisplayValue: displayValue.get,
    getAllByDisplayValue: displayValue.getAll,
    queryByDisplayValue: displayValue.query,
    queryAllByDisplayValue: displayValue.queryAll,
    findByDisplayValue: displayValue.find,
    findAllByDisplayValue: displayValue.findAll,
  };
}

/**
 * Queries over the whole document.
 *
 * The document rather than the rendered container, because a dialog, a tooltip
 * and a toast are rendered into a portal outside it — and a test that could
 * not see them would be unable to assert on the components most likely to have
 * a bug.
 */
export const screen: Queries = queriesFor(() => bodyOf());

/** The same queries, restricted to one element's subtree. */
export function within(element: Element): Queries {
  return queriesFor(() => element);
}
