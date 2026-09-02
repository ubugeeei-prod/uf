// @flow
//
// `@uniflowed/pwa`.

import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/pwa";

export type PwaConfig = {
  +name: string,
  +shortName?: string,
  +cache?: "opt-in",
};

export function definePwa(config: PwaConfig): PwaConfig {
  return nativeRuntimeRequired(MODULE, "definePwa");
}

export component Manifest(config: PwaConfig) {
  return nativeRuntimeRequired(MODULE, "Manifest");
}

export component ServiceWorker(config: PwaConfig) {
  return nativeRuntimeRequired(MODULE, "ServiceWorker");
}
