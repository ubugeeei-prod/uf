// @flow
//
// `@uniflowed/react-compiler`.

import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/react-compiler";

export type ReactCompilerMode = "syntax";

export interface ReactCompilerConfig {
  +enabled: boolean,
  +mode: ReactCompilerMode,
}

/** Default configuration: syntax mode, enabled. */
export const syntaxMode: ReactCompilerConfig = {
  enabled: true,
  mode: "syntax",
};

export function compiler(
  config?: $Shape<ReactCompilerConfig>,
): ReactCompilerConfig {
  return nativeRuntimeRequired(MODULE, "compiler");
}
