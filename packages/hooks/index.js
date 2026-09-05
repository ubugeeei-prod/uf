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
//
// # How the package is laid out
//
// Six modules beside this one, split by what a hook's subject is — because
// that is the question a reader looking for one actually asks:
//
// - `lifecycle.js` — the component itself: mounted, previous, run once.
// - `state.js` — a value the component owns, with the operations that suit it.
// - `timing.js` — when something runs: intervals, timeouts, debounce,
//   throttle.
// - `async.js` — one promise, and the stale-response bug.
// - `browser.js` — the ambient environment: viewport, connection, preferences.
// - `dom.js` — one element the caller holds a ref to: listen, measure,
//   observe.
//
// The two that are easiest to confuse are the last two, so each says so in its
// own header: `browser.js` needs no ref because there is one browser, and
// `dom.js` needs one because there are as many answers as there are elements.
//
// They sit here rather than under an `internal/`, and each has its own
// subpath. Every name in them is exported from this file, so calling them
// internal would have described nothing true, and it cost a reader a directory
// hop to reach the first line of code. `internal/` is for a module consumers
// must not reach; this package has none.
//
// `lifecycle.js` is the one the others import — `timing.js`, `state.js` and
// `dom.js` all want `useStableCallback` — and it is still a subject rather
// than a bag of shared helpers. A hook goes there because it is about the
// component's life, not because more than one file wanted it.

export type { Async } from "./async.js";

export { useAsync } from "./async.js";
export {
  useIsomorphicLayoutEffect,
  useMount,
  useMounted,
  usePrevious,
  useRerender,
  useStableCallback,
  useUnmount,
} from "./lifecycle.js";
export {
  useDebouncedCallback,
  useDebouncedValue,
  useInterval,
  useThrottledCallback,
  useTimeout,
} from "./timing.js";
export {
  useDocumentVisible,
  useMediaQuery,
  useOnline,
  usePrefersReducedMotion,
  usePreferredColorScheme,
  useWindowSize,
} from "./browser.js";
export {
  useClickOutside,
  useElementRef,
  useElementSize,
  useEventListener,
  useFocusWithin,
  useHover,
  useIntersecting,
} from "./dom.js";
export { useCounter, useStorage, useToggle } from "./state.js";
