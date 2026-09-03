// @flow
//
// `@uniflowed/hooks`: the hooks a React application writes anyway.
//
// Not a large collection. Every hook here is one that people write by hand in
// every project and get subtly wrong in the same way each time — a timer that
// calls a stale closure, a subscription re-established on every keystroke, a
// slow request overwriting a fast one, persisted state that differs between the
// server render and the first paint.
//
// # Prerendering is the constraint that shapes the surface
//
// uf prerenders every static route, so each of these runs once where there is
// no `window`. The browser hooks are built on `useSyncExternalStore`, which
// takes the server's value as a separate argument — so what a prerender sees is
// *stated* rather than being whatever a `typeof window` check fell through to,
// and React reads the value when it commits rather than when it renders, which
// is what stops a media query that changes mid-render from tearing.
//
// Where there is no honest default, the caller supplies one: a page that hides
// its sidebar under 48rem wants `false` on the server and one that renders a
// mobile menu wants `true`, and a library cannot know which.

export type { Async } from "./internal/async.js";

export { useAsync } from "./internal/async.js";
export {
  useIsomorphicLayoutEffect,
  useMount,
  useMounted,
  usePrevious,
  useRerender,
  useStableCallback,
  useUnmount,
} from "./internal/lifecycle.js";
export {
  useDebouncedCallback,
  useDebouncedValue,
  useInterval,
  useThrottledCallback,
  useTimeout,
} from "./internal/timing.js";
export {
  useDocumentVisible,
  useMediaQuery,
  useOnline,
  usePrefersReducedMotion,
  usePreferredColorScheme,
  useWindowSize,
} from "./internal/browser.js";
export {
  useClickOutside,
  useElementRef,
  useElementSize,
  useEventListener,
  useFocusWithin,
  useHover,
  useIntersecting,
} from "./internal/element.js";
export { useCounter, useStorage, useToggle } from "./internal/state.js";
