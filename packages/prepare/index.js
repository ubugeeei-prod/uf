// @flow
//
// `@uniflowed/prepare`.

import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/prepare";

export type PrepareStep =
  | "discover-staged-files"
  | "generate-router-types"
  | "generate-server-action-types"
  | "generate-validator-types"
  | "run-lint"
  | "run-format-check";

export type PreparePlan = {
  +lintStagedCompatible: boolean,
  +codeGenerator: boolean,
  +cache: "opt-in",
  +steps: $ReadOnlyArray<PrepareStep>,
};

export function prepare(): PreparePlan {
  return nativeRuntimeRequired(MODULE, "prepare");
}

export function lintStaged(): PrepareStep {
  return nativeRuntimeRequired(MODULE, "lintStaged");
}

export function codegen(): $ReadOnlyArray<PrepareStep> {
  return nativeRuntimeRequired(MODULE, "codegen");
}
