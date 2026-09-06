// @flow
//
// `@uniflowed/prepare`.

import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/prepare";

// The five steps `uf prepare` runs, in the order it runs them. Every one of
// them does something; the list is not a description of an intent.
//
// `"generate-validator-types"` was here and is gone. It was never implemented
// and it should not be: a validator schema is a runtime value, and the value
// and error types a form wants are already computed by the checker from
// `@uniflowed/validator/infer` — `InferOutput<typeof Account>` — with no
// generated file to disagree with the schema it came from. The error half
// cannot be generated at all, because Flow has no template-literal types and
// a form's errors are keyed by field path. `crates/uf_prepare/src/lib.rs`
// carries the long version.
export type PrepareStep =
  | "discover-staged-files"
  | "generate-router-types"
  | "generate-server-action-types"
  | "run-lint"
  | "run-format-check";

export type PreparePlan = {
  readonly lintStagedCompatible: boolean,
  readonly codeGenerator: boolean,
  readonly writeGeneratedFiles: boolean,
  readonly cache: "opt-in",
  readonly steps: $ReadOnlyArray<PrepareStep>,
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
