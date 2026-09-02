// @flow
//
// `@uniflowed/browser`.

import type { NativeHandle } from "@uniflowed/core/native";
import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/browser";

export type Viewport = {
  +name: string,
  +width: number,
  +height: number,
  +deviceScaleFactor?: number,
};

export type VisualSnapshot = {
  +storyId: string,
  +viewport: string,
  +baseline: string,
};

export opaque type BrowserPlan = NativeHandle<"@uniflowed/core/browser#BrowserPlan">;

export function browser(): BrowserPlan {
  return nativeRuntimeRequired(MODULE, "browser");
}

export function viewport(viewport: Viewport): BrowserPlan {
  return nativeRuntimeRequired(MODULE, "viewport");
}

export function visit(path: string): BrowserPlan {
  return nativeRuntimeRequired(MODULE, "visit");
}

export function screenshot(name: string): BrowserPlan {
  return nativeRuntimeRequired(MODULE, "screenshot");
}
