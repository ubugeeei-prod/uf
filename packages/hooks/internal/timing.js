// @flow
//
// Timers that stop when the component does.
//
// Every one of these exists because the hand-written version leaks: a
// `setInterval` in a `useEffect` whose dependency array includes the callback
// is torn down and restarted on every render, and one without the callback in
// the array calls a stale closure forever. `useStableCallback` removes the
// choice — the timer is set once and always calls the current body.

import { useEffect, useRef, useState } from "@uniflowed/react";

import { useStableCallback } from "./lifecycle.js";

/**
 * Call `body` every `millis`, or not at all when `millis` is null.
 *
 * Null rather than a separate `enabled` flag because "no interval" and "an
 * interval of nothing" are the same thing, and one argument cannot disagree
 * with itself.
 */
export function useInterval(body: () => mixed, millis: number | null): void {
  const stable = useStableCallback(body);
  useEffect(() => {
    if (millis == null) {
      return;
    }
    const id = setInterval(stable, millis);
    return () => clearInterval(id);
  }, [stable, millis]);
}

/** Call `body` once after `millis`, or not at all when `millis` is null. */
export function useTimeout(body: () => mixed, millis: number | null): void {
  const stable = useStableCallback(body);
  useEffect(() => {
    if (millis == null) {
      return;
    }
    const id = setTimeout(stable, millis);
    return () => clearTimeout(id);
  }, [stable, millis]);
}

/**
 * `value`, but only after it has stopped changing for `millis`.
 *
 * The classic use is a search box: the query updates on every keystroke and
 * the request should not.
 */
export function useDebouncedValue<T>(value: T, millis: number): T {
  const [settled, setSettled] = useState(value);

  useEffect(() => {
    const id = setTimeout(() => setSettled(value), millis);
    return () => clearTimeout(id);
  }, [value, millis]);

  return settled;
}

/**
 * A callback that runs at most once per `millis`.
 *
 * Leading edge: the first call goes through immediately and later ones inside
 * the window are dropped, which is what a scroll or resize handler wants —
 * the trailing-edge version would make the first paint late.
 */
export function useThrottledCallback<TArgs extends $ReadOnlyArray<mixed>>(
  body: (...args: TArgs) => mixed,
  millis: number,
): (...args: TArgs) => void {
  const stable = useStableCallback(body);
  const last = useRef(0);

  return useStableCallback((...args: TArgs) => {
    const now = Date.now();
    if (now - last.current >= millis) {
      last.current = now;
      stable(...args);
    }
  });
}

/**
 * A callback that runs `millis` after the last time it was asked to.
 *
 * Trailing edge, and it cancels itself at unmount — the version people write
 * calls `setState` on a component that is gone.
 */
export function useDebouncedCallback<TArgs extends $ReadOnlyArray<mixed>>(
  body: (...args: TArgs) => mixed,
  millis: number,
): (...args: TArgs) => void {
  const stable = useStableCallback(body);
  const timer = useRef<TimeoutID | null>(null);

  useEffect(
    () => () => {
      if (timer.current != null) {
        clearTimeout(timer.current);
      }
    },
    [],
  );

  return useStableCallback((...args: TArgs) => {
    if (timer.current != null) {
      clearTimeout(timer.current);
    }
    timer.current = setTimeout(() => {
      timer.current = null;
      stable(...args);
    }, millis);
  });
}
