// @flow
//
// `@uniflowed/hooks`.

import { nativeRuntimeRequired } from "./internal/native-runtime.js";

const MODULE = "@uniflowed/core/hooks";

export function useAsync<T>(
  body: () => Promise<T>,
  deps: $ReadOnlyArray<mixed>,
): {
  +value: null | T,
  +error: null | Error,
  +pending: boolean,
} {
  return nativeRuntimeRequired(MODULE, "useAsync");
}

export function useDebouncedValue<T>(value: T, delayMs: number): T {
  return nativeRuntimeRequired(MODULE, "useDebouncedValue");
}

export function useEvent<TEvent>(
  handler: (event: TEvent) => mixed,
): (event: TEvent) => void {
  return nativeRuntimeRequired(MODULE, "useEvent");
}

export function useInterval(
  callback: () => mixed,
  delayMs: null | number,
): void {
  return nativeRuntimeRequired(MODULE, "useInterval");
}

export function useIsomorphicLayoutEffect(
  setup: () => void | (() => void),
  deps: $ReadOnlyArray<mixed>,
): void {
  return nativeRuntimeRequired(MODULE, "useIsomorphicLayoutEffect");
}

export function useLocalStorage<T>(
  key: string,
  initial: T,
): [T, (next: T) => void] {
  return nativeRuntimeRequired(MODULE, "useLocalStorage");
}

export function useMediaQuery(query: string): boolean {
  return nativeRuntimeRequired(MODULE, "useMediaQuery");
}

export function useMounted(): boolean {
  return nativeRuntimeRequired(MODULE, "useMounted");
}

export function usePrevious<T>(value: T): void | T {
  return nativeRuntimeRequired(MODULE, "usePrevious");
}

export function useStableCallback<TArgs: $ReadOnlyArray<mixed>, TReturn>(
  callback: (...TArgs) => TReturn,
): (...TArgs) => TReturn {
  return nativeRuntimeRequired(MODULE, "useStableCallback");
}

export function useServerValue<T>(value: T): T {
  return nativeRuntimeRequired(MODULE, "useServerValue");
}
