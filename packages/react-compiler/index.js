// @flow
//
// `@uniflowed/react-compiler`.

import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/react-compiler";

export type ReactCompilerMode = "syntax";
export type ReactCompilerImplementation = "official-rust";

export interface ReactCompilerConfig {
  readonly enabled: boolean,
  readonly implementation: ReactCompilerImplementation,
  readonly mode: ReactCompilerMode,
}

/** Default configuration: syntax mode, enabled. */
export const syntaxMode: ReactCompilerConfig = {
  enabled: true,
  implementation: "official-rust",
  mode: "syntax",
};

export function compiler(
  config?: Partial<ReactCompilerConfig>,
): ReactCompilerConfig {
  return nativeRuntimeRequired(MODULE, "compiler");
}
