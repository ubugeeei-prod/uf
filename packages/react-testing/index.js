// @flow
//
// `@uniflowed/react-testing`: React Testing Library's surface, over a real DOM.
//
// The library's premise is that a test should reach for an element the way a
// person does — by the text on it, by the role it plays, by the label next to
// it — so a test keeps passing when the markup is refactored and stops passing
// when the thing a user relies on breaks. That premise is worth keeping, so
// this is the same shape: `render`, `screen`, `fireEvent`, `userEvent`,
// `waitFor`.
//
// It used to be a declaration whose every function threw. `uf test` runs on
// Node.js, Bun or Deno and none of them has a DOM, so one is installed on
// first render — which is why a component test needs nothing configured.
//
// # Queries
//
// Every query is `getBy`, `queryBy`, `findBy`, `getAllBy`, `queryAllBy` or
// `findAllBy` over the same matcher, and the choice says what the test means:
// `getBy` asserts presence now, `queryBy` is for asking about absence, and
// `findBy` waits. They are on `screen`, which searches the whole document so a
// portal is found, and on the result of `render`, which searches only what it
// mounted.

export type { Matcher, MatcherOptions } from "./internal/queries.js";
export type { RenderResult } from "./internal/render.js";
export type { Queries } from "./internal/screen.js";

export { normalize, accessibleName, roleOf } from "./internal/queries.js";
export { actively as act, cleanup, render, waitFor } from "./internal/render.js";
export { dispatch, fireEvent, userEvent } from "./internal/events.js";
export { screen, within } from "./internal/screen.js";
