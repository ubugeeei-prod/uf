// @flow
//
// `@uniflowed/story`.

import type * as React from "@uniflowed/react";
import type { BrowserPlan } from "@uniflowed/browser";
import type { MockRegistry } from "@uniflowed/mock";
import type { NativeHandle } from "@uniflowed/core/native";
import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/story";

export opaque type Storybook = NativeHandle<"@uniflowed/core/story#Storybook">;

export type StoryVariant<Props: {...}> = {
  +name: string,
  +props: Props,
};

export function story<Props: {...}>(
  id: string,
  render: component(...Props) renders React.Node,
): Storybook {
  return nativeRuntimeRequired(MODULE, "story");
}

export function variant<Props: {...}>(
  storybook: Storybook,
  variant: StoryVariant<Props>,
): Storybook {
  return nativeRuntimeRequired(MODULE, "variant");
}

export function withMocks(
  storybook: Storybook,
  mocks: MockRegistry,
): Storybook {
  return nativeRuntimeRequired(MODULE, "withMocks");
}

export function withBrowser(
  storybook: Storybook,
  browser: BrowserPlan,
): Storybook {
  return nativeRuntimeRequired(MODULE, "withBrowser");
}
