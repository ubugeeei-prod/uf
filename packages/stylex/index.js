// @flow
//
// `@uniflowed/stylex`.

import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/stylex";

export interface StyleXSheet {
  readonly [string]: mixed;
}

export interface StyleX {
  create<T extends { readonly [string]: mixed }>(styles: T): T;
  props(...styles: $ReadOnlyArray<mixed>): { readonly [string]: mixed };
  defineVars<T extends { readonly [string]: string | number }>(tokens: T): T;
  createTheme<T extends { readonly [string]: string | number }>(tokens: T): T;
}

export function defineVars<T extends { readonly [string]: string | number }>(tokens: T): T {
  return nativeRuntimeRequired(MODULE, "defineVars");
}

export function createTheme<T extends { readonly [string]: string | number }>(tokens: T): T {
  return nativeRuntimeRequired(MODULE, "createTheme");
}

export const stylex: StyleX = {
  create<T extends { readonly [string]: mixed }>(styles: T): T {
    return nativeRuntimeRequired(MODULE, "stylex.create");
  },
  props(...styles: $ReadOnlyArray<mixed>): { readonly [string]: mixed } {
    return nativeRuntimeRequired(MODULE, "stylex.props");
  },
  defineVars,
  createTheme,
};
