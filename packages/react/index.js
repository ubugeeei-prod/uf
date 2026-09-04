// @flow
//
// `@uniflowed/react`: React, re-exported.
//
// This is the real `react` package, not a declaration of it. `export *` rather
// than a list of names, because the list was a second copy of React's export
// surface that someone had to keep in step with the first — and the failure
// mode of falling behind is an `undefined` an application finds at runtime,
// long after the name was added upstream.
//
// The Flow types come from Flow's own library definition for `react`, so
// `import type { Node } from "@uniflowed/react"` is exactly
// `import type { Node } from "react"`.
//
// # Which React
//
// A peer dependency, so the version is the project's and not uf's. A uf
// release does not get to decide when an application moves to the next React,
// and an application can pin or override one with its package manager's
// `resolutions` without waiting for uf — red line 5, which exists because
// `react-scripts` was the package that serialised exactly this.

export * from "react";

import * as React from "react";

/** The setter shape `useState` hands back. */
export type SetState<S> = S | ((previous: S) => S);

export { React };
export default React;
