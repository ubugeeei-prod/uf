// @flow
//
// `@uniflowed/lint`.

import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/lint";

export type RuleSeverity = "off" | "warn" | "error";

export type RuleContext = {
  +filename: string,
  +source: string,
};

export type RuleDiagnostic = {
  +rule: string,
  +message: string,
  +line: number,
  +column: number,
};

export type NativeRule = (
  context: RuleContext,
) => $ReadOnlyArray<RuleDiagnostic>;

export function defineRule(name: string, rule: NativeRule): NativeRule {
  return nativeRuntimeRequired(MODULE, "defineRule");
}

export function typeAwareRule(name: string, rule: NativeRule): NativeRule {
  return nativeRuntimeRequired(MODULE, "typeAwareRule");
}

export function reactRule(name: string, rule: NativeRule): NativeRule {
  return nativeRuntimeRequired(MODULE, "reactRule");
}
