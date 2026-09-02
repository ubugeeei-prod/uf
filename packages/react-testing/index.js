// @flow
//
// `@uniflowed/react-testing`.

import type * as React from "@uniflowed/react";
import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/react-testing";

export interface RenderResult {
  rerender(ui: React.Node): void,
  unmount(): void,
}

export interface Screen {
  getByText(text: string | RegExp): HTMLElement,
  findByText(text: string | RegExp): Promise<HTMLElement>,
  queryByText(text: string | RegExp): null | HTMLElement,
}

export const screen: Screen = {
  getByText(text: string | RegExp): HTMLElement {
    return nativeRuntimeRequired(MODULE, "screen.getByText");
  },
  findByText(text: string | RegExp): Promise<HTMLElement> {
    return nativeRuntimeRequired(MODULE, "screen.findByText");
  },
  queryByText(text: string | RegExp): null | HTMLElement {
    return nativeRuntimeRequired(MODULE, "screen.queryByText");
  },
};

export function render(ui: React.Node): RenderResult {
  return nativeRuntimeRequired(MODULE, "render");
}

export function waitFor<T>(body: () => T | Promise<T>): Promise<T> {
  return nativeRuntimeRequired(MODULE, "waitFor");
}

/**
 * Event helpers keyed by event name.
 *
 * The declared type is an open dictionary, so there is no finite set of keys to
 * materialise as raising functions the way `screen` does. The native runtime
 * replaces this object wholesale; until then a lookup yields `undefined`, and
 * calling it fails with a `TypeError` rather than the shared message.
 */
export const fireEvent: {
  [string]: (target: EventTarget, event?: mixed) => void,
} = {};

/** Same open-dictionary shape as `fireEvent`; see the note there. */
export const userEvent: {
  [string]: (...args: $ReadOnlyArray<mixed>) => Promise<void>,
} = {};
