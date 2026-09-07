// @flow
//
// `@uniflowed/hooks`: the hooks a React application writes anyway.
//
// Every hook here is one that people write by hand in every project and get
// subtly wrong in the same way each time — a timer that calls a stale closure,
// a subscription re-established on every keystroke, a slow request overwriting
// a fast one, persisted state that differs between the server render and the
// first paint, a shortcut that fires while somebody is typing, a dialog that
// unlocks the page while a second dialog is still open.
//
// VueUse is the benchmark, and most of what it offers is not here on purpose.
// Its `Reactivity`, `Watch` and `Array` categories exist because Vue's `ref`
// is a mutable box that has to be wrapped, unwrapped, synced and derived;
// React has no box, and `useMemo` over a plain array is the whole of
// `useArrayFilter`. Its `Component` category is Vue's template machinery —
// `templateRef`, `unrefElement`, `useVModel` — which is `ref` and props here.
// Porting either would have produced hooks whose only purpose was to look
// familiar. The "Readiness" section below says what is here, what is
// deliberately elsewhere in uf, and what is simply not built.
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
// Where there is exactly one honest answer, the caller is not asked. `useHash`
// takes no server value because the browser strips the fragment before the
// request goes out, so `""` is not a default standing in for something better —
// it is what the server knows, and offering an override would have invited a
// caller to state a value that cannot be true.
//
// The hooks built on effects rather than stores — everything in `dom.js`, and
// the three in `browser.js` whose first value only arrives in a callback — do
// nothing at all before hydration, and report the same starting value on both
// sides for that reason. Each module's header says which it is and why.
//
// # React's rules are the design, not a constraint on it
//
// Nothing here writes during a render, reads a ref during a render, or depends
// on a render having happened exactly once. Callbacks that cross into effects
// go through `useStableCallback`, whose ref is written in an insertion effect
// rather than in the body, so Strict Mode's double render and a render the
// React Compiler skips are both correct. Every effect's cleanup removes
// exactly what its body added, which is what makes Strict Mode's
// mount-unmount-mount balanced — including the module-level counters behind
// `useScrollLock` and `useStorage`.
//
// # How the package is laid out
//
// Nine modules beside this one, split by what a hook's subject is — because
// that is the question a reader looking for one actually asks:
//
// - `lifecycle.js` — the component itself: mounted, previous, run once.
// - `state.js` — a value the component owns, with the operations that suit it:
//   toggle, counter, list, set, cycle, undo/redo, storage.
// - `timing.js` — when something runs: intervals, timeouts, debounce,
//   throttle, frames, idleness, and the clock behind "3 minutes ago".
// - `render.js` — what a render has to fix rather than derive: the instant it
//   was made at, and the seed anything random on the page is drawn from.
// - `async.js` — one promise: its states, its abort signal, its retries.
// - `browser.js` — the ambient environment: viewport, scroll, connection,
//   position, permissions, preferences, the fragment in the address bar. Its
//   header carries the table of what every one of them renders before
//   hydration.
// - `dom.js` — one element the caller holds a ref to: listen, measure,
//   observe, press.
// - `keyboard.js` — what is being pressed: a chord, and a held key.
// - `channels.js` — a value that came from outside the page: another tab, the
//   system clipboard.
// - `events.js` — a stream the server is pushing, and where its reconnection
//   is. Not `channels.js`, whose boundary is deliberately everything except
//   the server, and not `@uniflowed/query`, because there is no cache entry
//   and nothing to revalidate: a connection is a third thing.
//
// The two that are easiest to confuse are `browser.js` and `dom.js`, so each
// says so in its own header: `browser.js` needs no ref because there is one
// browser, and `dom.js` needs one because there are as many answers as there
// are elements. `keyboard.js` is neither, which is why it is a third file
// rather than a corner of one of them, and its header says what the four hard
// parts of a shortcut are.
//
// They sit here rather than under an `internal/`, and each has its own
// subpath. Every name in them is exported from this file, so calling them
// internal would have described nothing true, and it cost a reader a directory
// hop to reach the first line of code. `internal/` is for a module consumers
// must not reach; this package has none.
//
// `lifecycle.js` and `browser.js` are the two the others import —
// `useStableCallback` and `browserWindow` — and both are still subjects rather
// than bags of shared helpers. A hook goes in `lifecycle.js` because it is
// about the component's life, never because more than one file wanted it.
//
// # Readiness
//
// **Implemented and tested.** The component's life; the state shapes; every
// timer, including the adaptive schedule behind `useTimeAgo`; `useAsync` with
// abort and retry; media queries, colour scheme, reduced motion, online,
// document visibility, window size and scroll, the address bar's fragment,
// scroll lock; element size,
// intersection, mutations, hover, focus-within, click-outside, long press,
// element scroll, the element as state; key chords and held keys; storage with
// cross-tab sync;
// broadcast channels; the clipboard; a server-sent event stream, including the
// one case the platform's own reconnection gives up on; the render anchor and
// the seeded stream in `render.js`. `tests/library/hooks.test.js` covers
// behaviour and cleanup, and `tests/library/hooks-ssr.test.js` renders the
// whole surface in a process that has no DOM at all.
//
// **Experimental.** `useGeolocation`, `useNetwork` and `usePermission`. The
// shapes are settled and the cleanup is right, but the browsers disagree about
// them more than the rest of this package does: Network Information is
// Chromium's alone, the Permissions API rejects rather than answers for names
// it does not know, and geolocation cannot be exercised end to end in a
// headless document — the tests cover the unsupported path, the subscription
// and its teardown, not a real fix. Treat the fields as advisory.
//
// **Not implemented, and not planned here.** Everything whose subject is
// somewhere else in uf: a request with a cache is `@uniflowed/query`, a form
// field is `@uniflowed/form`, an atom two routes read is `@uniflowed/state`, a
// rendered instant is `@uniflowed/web`'s `Time`, styling and dark mode are
// `@uniflowed/stylex`, and a virtual list or an infinite scroller is
// `@uniflowed/ui`. Beyond those: the device APIs VueUse wraps that a general
// application does not reach for — Bluetooth, gamepads, USB, speech, wake
// lock, screen capture, web workers, battery, vibration — are absent rather
// than shallow. A wrapper over one of those is three lines and a `supported`
// flag; what makes it worth shipping is knowing the failure modes, and this
// package does not yet.

export type { Async, AsyncOptions } from "./async.js";
export type {
  BrowserHistory,
  BrowserLocation,
  BrowserNavigator,
  BrowserWindow,
  EffectiveConnectionType,
  GeolocationReading,
  Geoposition,
  Network,
  NetworkConnection,
  NetworkMeasurement,
  PermissionAnswer,
  PermissionName,
  ScrollOffset,
  Size,
} from "./browser.js";
export type { UseBroadcastReturn, UseClipboardReturn } from "./channels.js";
export type { ListenerOptions, ListenerTarget, MutationOptions, Ref } from "./dom.js";
export type {
  EventSourceOptions,
  EventStreamStatus,
  ServerEvent,
  UseEventSourceReturn,
} from "./events.js";
export type { KeyComboOptions } from "./keyboard.js";
export type { RenderEnvelope } from "./render.js";
export type {
  UseCounterReturn,
  UseCycleReturn,
  UseListReturn,
  UseSetReturn,
  UseToggleReturn,
  UseUndoableReturn,
} from "./state.js";

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
  useAnimationFrame,
  useDebouncedCallback,
  useDebouncedValue,
  useIdle,
  useInterval,
  useNow,
  useThrottledCallback,
  useTimeAgo,
  useTimeout,
} from "./timing.js";
export {
  browserWindow,
  useDocumentVisible,
  useGeolocation,
  useHash,
  useMediaQuery,
  useNetwork,
  useOnline,
  usePermission,
  usePreferredColorScheme,
  usePrefersReducedMotion,
  useScrollLock,
  useSupported,
  useWindowScroll,
  useWindowSize,
} from "./browser.js";
export {
  useClickOutside,
  useElementRef,
  useElementSize,
  useElementState,
  useEventListener,
  useFocusWithin,
  useHover,
  useIntersecting,
  useLongPress,
  useMutationObserver,
  useScroll,
} from "./dom.js";
export { useKeyCombo, useKeyHeld } from "./keyboard.js";
export {
  RENDER_ID,
  RenderProvider,
  useRandom,
  useRenderEnvelope,
  useRenderTimeZone,
  useRenderedAt,
  useShuffled,
} from "./render.js";
export { useBroadcast, useClipboard } from "./channels.js";
export { useEventSource } from "./events.js";
export {
  useCounter,
  useCycle,
  useList,
  useSet,
  useStorage,
  useToggle,
  useUndoable,
} from "./state.js";
