// @flow
//
// The six forms of every query, generated once.
//
// Writing `getByText`, `queryByText`, `findByText`, `getAllByText`,
// `queryAllByText` and `findAllByText` by hand, for seven queries, is
// forty-two functions that differ in two decisions: whether finding nothing is
// an error, and whether to wait. So the two decisions are written once and the
// forty-two are derived — which also means a new query is one entry rather
// than six functions.

import {
  allByDisplayValue,
  allByLabelText,
  allByPlaceholderText,
  allByRole,
  allByTestId,
  allByText,
  queryFailure,
} from "./queries.js";
import type { Matcher, MatcherOptions } from "./queries.js";
import { documentOf } from "./dom.js";
import { waitFor } from "./render.js";

/** The queries available on `screen` and on `within(element)`. */
export type Queries = {
  readonly [string]: (matcher: mixed, options?: mixed) => any,
};

/** Every query, as the one function each needs. */
const FINDERS = {
  Text: allByText,
  Role: allByRole,
  LabelText: allByLabelText,
  PlaceholderText: allByPlaceholderText,
  TestId: allByTestId,
  DisplayValue: allByDisplayValue,
};

/**
 * The six forms of one finder, bound to a root.
 *
 * `getBy` fails when there is not exactly one, and says how many it saw and
 * what the markup looked like, because "found 3 elements" and "found nothing"
 * are different bugs and a test that reports neither wastes the reader's time.
 */
function forms(name: string, find: Function, root: () => ParentNode): { [string]: Function } {
  const all = (matcher: Matcher, options?: MatcherOptions) => find(root(), matcher, options);

  return {
    [`getAllBy${name}`]: (matcher: Matcher, options?: MatcherOptions) => {
      const found = all(matcher, options);
      if (found.length === 0) {
        throw queryFailure(`getAllBy${name}`, matcher, root(), 0);
      }
      return found;
    },
    [`queryAllBy${name}`]: all,
    [`getBy${name}`]: (matcher: Matcher, options?: MatcherOptions) => {
      const found = all(matcher, options);
      if (found.length !== 1) {
        throw queryFailure(`getBy${name}`, matcher, root(), found.length);
      }
      return found[0];
    },
    [`queryBy${name}`]: (matcher: Matcher, options?: MatcherOptions) => {
      const found = all(matcher, options);
      if (found.length > 1) {
        throw queryFailure(`queryBy${name}`, matcher, root(), found.length);
      }
      return found[0] ?? null;
    },
    [`findBy${name}`]: (matcher: Matcher, options?: MatcherOptions) =>
      waitFor(() => {
        const found = all(matcher, options);
        if (found.length !== 1) {
          throw queryFailure(`findBy${name}`, matcher, root(), found.length);
        }
        return found[0];
      }),
    [`findAllBy${name}`]: (matcher: Matcher, options?: MatcherOptions) =>
      waitFor(() => {
        const found = all(matcher, options);
        if (found.length === 0) {
          throw queryFailure(`findAllBy${name}`, matcher, root(), 0);
        }
        return found;
      }),
  };
}

function queriesFor(root: () => ParentNode): Queries {
  const queries = {};
  for (const name of Object.keys(FINDERS)) {
    Object.assign(queries, forms(name, (FINDERS as any)[name], root));
  }
  return queries as any;
}

/**
 * Queries over the whole document.
 *
 * The document rather than the rendered container, because a dialog, a tooltip
 * and a toast are rendered into a portal outside it — and a test that could
 * not see them would be unable to assert on the components most likely to have
 * a bug.
 */
export const screen: Queries = queriesFor(() => documentOf().body);

/** The same queries, restricted to one element's subtree. */
export function within(element: ParentNode): Queries {
  return queriesFor(() => element);
}
