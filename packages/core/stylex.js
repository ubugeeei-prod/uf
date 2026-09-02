// @flow
//
// `@uniflowed/stylex`.

import { nativeRuntimeRequired } from "./internal/native-runtime.js";

const MODULE = "@uniflowed/core/stylex";

export interface StyleXSheet {
  +[string]: mixed,
}

export interface StyleX {
  create<T: { +[string]: mixed }>(styles: T): T,
  props(...styles: $ReadOnlyArray<mixed>): { +[string]: mixed },
  defineVars<T: { +[string]: string | number }>(tokens: T): T,
  createTheme<T: { +[string]: string | number }>(tokens: T): T,
}

export function defineVars<T: { +[string]: string | number }>(tokens: T): T {
  return nativeRuntimeRequired(MODULE, "defineVars");
}

export function createTheme<T: { +[string]: string | number }>(tokens: T): T {
  return nativeRuntimeRequired(MODULE, "createTheme");
}

export const stylex: StyleX = {
  create<T: { +[string]: mixed }>(styles: T): T {
    return nativeRuntimeRequired(MODULE, "stylex.create");
  },
  props(...styles: $ReadOnlyArray<mixed>): { +[string]: mixed } {
    return nativeRuntimeRequired(MODULE, "stylex.props");
  },
  defineVars,
  createTheme,
};
