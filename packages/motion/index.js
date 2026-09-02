// @flow
//
// `@uniflowed/motion`.

import type * as React from "@uniflowed/react";
import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/motion";

export type MotionEngine = "uf-native";
export type MotionProperty = "transform" | "opacity" | "color";
export type MotionEasing = "linear" | "out" | "spring";

export type MotionTrack = {
  +id: string,
  +property: MotionProperty,
  +durationMs: number,
  +easing?: MotionEasing,
};

export type MotionContract = {
  +engine: MotionEngine,
  +tracks: $ReadOnlyArray<MotionTrack>,
  +compilerSafe: true,
  +serverComponentSafe: true,
  +reducedMotionDefault: true,
};

export function motion(contract?: MotionContract): MotionContract {
  return nativeRuntimeRequired(MODULE, "motion");
}

export function animate(track: MotionTrack): MotionContract {
  return nativeRuntimeRequired(MODULE, "animate");
}

export function timeline(
  tracks: $ReadOnlyArray<MotionTrack>,
): MotionContract {
  return nativeRuntimeRequired(MODULE, "timeline");
}

export function spring(
  id: string,
  property: MotionProperty,
  durationMs: number,
): MotionTrack {
  return nativeRuntimeRequired(MODULE, "spring");
}

export function reducedMotion(children: React.Node): React.Node {
  return nativeRuntimeRequired(MODULE, "reducedMotion");
}
