// @flow
//
// `@uniflowed/hooks/lifecycle`: where a component is in its life.
//
// The one that matters most is `useStableCallback`. A callback recreated every
// render is the single most common cause of a React performance problem and of
// a subscription that tears itself down and sets itself up on every keystroke —
// and the usual fix, listing the callback in a dependency array, spreads the
// problem to every hook that takes it. A stable identity that always calls the
// latest closure fixes it once.
//
// # What belongs in this module
//
// A hook whose subject is the component itself: has it mounted, is it still
// mounted, what did it render last time, run this once, run this on the way
// out, and the two effect-shaped primitives the rest of the package needs
// (`useIsomorphicLayoutEffect`, `useStableCallback`). None of them reads
// anything outside React.
//
// It is also the one module here the others import, which is a consequence and
// not the reason: `timing.js`, `state.js` and `dom.js` all need a stable
// callback. That does not make this a "utils" or a "common" — it has a subject
// of its own, and a hook lands here because it is about the component's life,
// never because two other files happened to want it.

import {
  useCallback,
  useEffect,
  useInsertionEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "@uniflowed/react";

/**
 * `useLayoutEffect` in the browser, `useEffect` on the server.
 *
 * uf prerenders every static route, and React warns that `useLayoutEffect`
 * does nothing during a server render — correctly, because there is no layout
 * to read. Every hook here that measures or subscribes uses this, so a hook is
 * not a reason a page cannot be prerendered.
 */
export const useIsomorphicLayoutEffect: typeof useLayoutEffect =
  typeof globalThis.document === "undefined" ? useEffect : useLayoutEffect;

/**
 * A callback whose identity never changes and whose body is always the latest.
 *
 * This is the `useEvent` shape from React's own RFC. The ref is written in an
 * insertion effect rather than in the render, because writing it during render
 * makes the callback's behaviour depend on whether that render was thrown away
 * — and it is written before any layout effect runs, so a subscription set up
 * in one already sees the current body.
 */
export function useStableCallback<TArgs extends $ReadOnlyArray<mixed>, TReturn>(
  callback: (...args: TArgs) => TReturn,
): (...args: TArgs) => TReturn {
  const latest = useRef(callback);

  useInsertionEffect(() => {
    latest.current = callback;
  }, [callback]);

  return useCallback((...args: TArgs) => latest.current(...args), []);
}

/** The value from the previous render, or `undefined` on the first. */
export function usePrevious<T>(value: T): T | void {
  const previous = useRef<T | void>(undefined);
  useEffect(() => {
    previous.current = value;
  }, [value]);
  return previous.current;
}

/**
 * Whether the component has mounted.
 *
 * For the case where a value differs between server and client and rendering
 * the client's on the first pass would be a hydration mismatch: render the
 * server's, then switch.
 */
export function useMounted(): boolean {
  const [mounted, setMounted] = useState(false);
  useEffect(() => {
    setMounted(true);
  }, []);
  return mounted;
}

/** Run `body` once, after mount. */
export function useMount(body: () => mixed): void {
  const stable = useStableCallback(body);
  useEffect(() => {
    stable();
  }, [stable]);
}

/** Run `body` once, at unmount. */
export function useUnmount(body: () => mixed): void {
  const stable = useStableCallback(body);
  useEffect(() => () => void stable(), [stable]);
}

/**
 * Force a re-render.
 *
 * A counter rather than a boolean, because two renders in a row must both
 * change the state or React drops the second.
 */
export function useRerender(): () => void {
  const [, setTick] = useState(0);
  return useCallback(() => setTick((tick) => tick + 1), []);
}
