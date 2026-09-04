// @flow
//
// `@uniflowed/browser`.

import type { NativeHandle } from "@uniflowed/core/native";
import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/browser";

export type Viewport = {
  readonly name: string,
  readonly width: number,
  readonly height: number,
  readonly deviceScaleFactor?: number,
};

export type VisualSnapshot = {
  readonly storyId: string,
  readonly viewport: string,
  readonly baseline: string,
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
