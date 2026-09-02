// @flow
//
// `@uniflowed/lib`: reflection over the native module registry that
// `crates/uf_lib/src/lib.rs` owns.

import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/lib";

export type NativeModuleKind =
  | "data"
  | "effect"
  | "framework"
  | "hooks"
  | "runtime"
  | "std"
  | "style"
  | "testing"
  | "ui";

export type NativeModule = {
  +specifier: string,
  +kind: NativeModuleKind,
  +stability: "experimental" | "planned" | "stable",
  +flowExports: $ReadOnlyArray<string>,
};

/**
 * Version of the uf runtime that answers this package.
 *
 * The native runtime replaces the binding; outside it the version is unknown,
 * and an empty string is the only honest answer a data property can give.
 */
export const version: string = "";

export function modules(): $ReadOnlyArray<NativeModule> {
  return nativeRuntimeRequired(MODULE, "modules");
}
