// @flow
//
// `@uniflowed/jsx-runtime`.
//
// The automatic JSX runtime. Nothing imports this by hand: `uf build` injects
// the import when it lowers a module's JSX, which is why the local names it
// binds are `_jsx`, `_jsxs` and `_Fragment` rather than anything a person
// would type.

import { nativeRuntimeRequired } from "@uniflowed/core/native";
import type { Node } from "@uniflowed/react";

const MODULE = "@uniflowed/core/jsx-runtime";

/**
 * Props an element carries, with its children under the reserved name the
 * runtime reads them from.
 */
export type JsxProps = { +children?: Node, ... };

/**
 * Create an element with zero or one child.
 *
 * `key` is a third argument rather than a prop: the compiler lifts it out of
 * the props object so that two elements differing only by key are still one
 * call site.
 */
export function jsx(type: mixed, props: JsxProps, key?: mixed): Node {
  return nativeRuntimeRequired(MODULE, "jsx");
}

/**
 * Create an element whose children are a list.
 *
 * Identical to `jsx` except that the runtime may skip its own check that the
 * children array is static, because the compiler already knows it is.
 */
export function jsxs(type: mixed, props: JsxProps, key?: mixed): Node {
  return nativeRuntimeRequired(MODULE, "jsxs");
}

/** The type `<>…</>` lowers to. */
export const Fragment: mixed = Symbol.for("uf.fragment");
