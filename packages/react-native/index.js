// @flow
//
// `@uniflowed/react-native`.

import type * as React from "@uniflowed/react";
import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/react-native";

export component View(...props: $ReadOnly<{ readonly children?: React.Node }>) {
  return nativeRuntimeRequired(MODULE, "View");
}

export component Text(...props: $ReadOnly<{ readonly children?: React.Node }>) {
  return nativeRuntimeRequired(MODULE, "Text");
}

/**
 * Host description. The native runtime replaces this object with the real host;
 * outside it the only truthful value is the generic `native` target, and
 * `select` raises rather than silently picking a branch.
 */
export const Platform: {
  readonly OS: "ios" | "android" | "web" | "native",
  readonly select: <T>(
    options: $ReadOnly<{
      readonly ios?: T,
      readonly android?: T,
      readonly web?: T,
      readonly native?: T,
    }>,
  ) => T | void,
} = {
  OS: "native",
  select: <T>(
    options: $ReadOnly<{
      readonly ios?: T,
      readonly android?: T,
      readonly web?: T,
      readonly native?: T,
    }>,
  ): T | void => nativeRuntimeRequired(MODULE, "Platform.select"),
};
