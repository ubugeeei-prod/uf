// @flow
//
// `@uniflowed/server`.

import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/server";

export opaque type ServerAction<TArgs extends $ReadOnlyArray<mixed>, TReturn> = {
  readonly __ufNative: "@uniflowed/core/server#ServerAction",
  __ufArgs: TArgs,
  __ufReturn: TReturn,
};

export function serverAction<TArgs extends $ReadOnlyArray<mixed>, TReturn>(
  action: (...TArgs) => Promise<TReturn> | TReturn,
): (...TArgs) => Promise<TReturn> {
  return nativeRuntimeRequired(MODULE, "serverAction");
}

export function headers(): { get(name: string): null | string } {
  return nativeRuntimeRequired(MODULE, "headers");
}

export function cookies(): { get(name: string): null | string } {
  return nativeRuntimeRequired(MODULE, "cookies");
}

export function cache<T>(key: string, body: () => T | Promise<T>): Promise<T> {
  return nativeRuntimeRequired(MODULE, "cache");
}

export function draftMode(): {
  readonly isEnabled: boolean,
  readonly enable: () => void,
  readonly disable: () => void,
} {
  return nativeRuntimeRequired(MODULE, "draftMode");
}

export function after(callback: () => mixed | Promise<mixed>): void {
  return nativeRuntimeRequired(MODULE, "after");
}
