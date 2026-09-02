// @flow
//
// `@uniflowed/react-native`.

import type * as React from "./react.js";
import { nativeRuntimeRequired } from "./internal/native-runtime.js";

const MODULE = "@uniflowed/core/react-native";

export component View(...props: $ReadOnly<{ +children?: React.Node }>) {
  return nativeRuntimeRequired(MODULE, "View");
}

export component Text(...props: $ReadOnly<{ +children?: React.Node }>) {
  return nativeRuntimeRequired(MODULE, "Text");
}

/**
 * Host description. The native runtime replaces this object with the real host;
 * outside it the only truthful value is the generic `native` target, and
 * `select` raises rather than silently picking a branch.
 */
export const Platform: {
  +OS: "ios" | "android" | "web" | "native",
  +select: <T>(
    options: $ReadOnly<{ +ios?: T, +android?: T, +web?: T, +native?: T }>,
  ) => T | void,
} = {
  OS: "native",
  select: <T>(
    options: $ReadOnly<{ +ios?: T, +android?: T, +web?: T, +native?: T }>,
  ): T | void => nativeRuntimeRequired(MODULE, "Platform.select"),
};
