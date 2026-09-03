// @flow
//
// Reading the browser, safely on a server.
//
// uf prerenders every static route, so each of these runs once where there is
// no `window`. `useSyncExternalStore` is what makes that correct rather than
// guarded: it takes a server snapshot as a separate argument, so the value
// used during prerender is stated rather than being whatever a `typeof window`
// check happened to fall through to. It also means React reads the value at
// the moment it commits, which is what stops a media query changing between
// render and paint from tearing.

import { useCallback, useSyncExternalStore } from "@uniflowed/react";

/** Whether there is a document to read at all. */
function inBrowser(): boolean {
  return typeof globalThis.document !== "undefined";
}

/**
 * The window these hooks listen to.
 *
 * In a browser `globalThis` *is* the window, so `globalThis.addEventListener`
 * looks correct. It is not correct anywhere a document has been installed onto
 * another host's global — which is every uf test process, where `globalThis` is
 * Node's and has no `addEventListener` at all. Ask the window for its own
 * methods and both cases work.
 */
function windowOf(): any {
  return globalThis.window ?? globalThis;
}

/**
 * Whether a media query matches.
 *
 * `serverValue` is what a prerender should assume, and it has no honest
 * default — a page that hides a sidebar under 48rem wants `false` on the
 * server, and one that renders a mobile menu wants `true`. So the caller says.
 */
export function useMediaQuery(query: string, serverValue: boolean = false): boolean {
  const subscribe = useCallback(
    (notify: () => void) => {
      if (!inBrowser() || typeof windowOf().matchMedia !== "function") {
        return () => {};
      }
      const list = windowOf().matchMedia(query);
      list.addEventListener("change", notify);
      return () => list.removeEventListener("change", notify);
    },
    [query],
  );

  return useSyncExternalStore(
    subscribe,
    () =>
      inBrowser() && typeof windowOf().matchMedia === "function"
        ? windowOf().matchMedia(query).matches
        : serverValue,
    () => serverValue,
  );
}

/** The reader's colour-scheme preference. */
export function usePreferredColorScheme(
  serverValue: "light" | "dark" = "light",
): "light" | "dark" {
  return useMediaQuery("(prefers-color-scheme: dark)", serverValue === "dark")
    ? "dark"
    : "light";
}

/** Whether the reader has asked for less motion. */
export function usePrefersReducedMotion(serverValue: boolean = false): boolean {
  return useMediaQuery("(prefers-reduced-motion: reduce)", serverValue);
}

/** Whether the browser thinks it is online. */
export function useOnline(serverValue: boolean = true): boolean {
  const subscribe = useCallback((notify: () => void) => {
    if (!inBrowser()) {
      return () => {};
    }
    const win = windowOf();
    win.addEventListener("online", notify);
    win.addEventListener("offline", notify);
    return () => {
      win.removeEventListener("online", notify);
      win.removeEventListener("offline", notify);
    };
  }, []);

  return useSyncExternalStore(
    subscribe,
    () => (inBrowser() ? (windowOf().navigator?.onLine ?? true) : serverValue),
    () => serverValue,
  );
}

/** Whether the document is the one the reader is looking at. */
export function useDocumentVisible(serverValue: boolean = true): boolean {
  const subscribe = useCallback((notify: () => void) => {
    if (!inBrowser()) {
      return () => {};
    }
    globalThis.document.addEventListener("visibilitychange", notify);
    return () => globalThis.document.removeEventListener("visibilitychange", notify);
  }, []);

  return useSyncExternalStore(
    subscribe,
    () => (inBrowser() ? globalThis.document.visibilityState !== "hidden" : serverValue),
    () => serverValue,
  );
}

/** The size of the viewport. */
export function useWindowSize(serverValue?: {| +width: number, +height: number |}): {|
  +width: number,
  +height: number,
|} {
  const fallback = serverValue ?? { width: 0, height: 0 };

  const subscribe = useCallback((notify: () => void) => {
    if (!inBrowser()) {
      return () => {};
    }
    const win = windowOf();
    win.addEventListener("resize", notify);
    return () => win.removeEventListener("resize", notify);
  }, []);

  // A string snapshot, because `useSyncExternalStore` compares snapshots by
  // identity: returning a fresh object every time would re-render on every
  // check, which is an infinite loop React reports rather than tolerates.
  const packed = useSyncExternalStore(
    subscribe,
    () =>
      inBrowser()
        ? `${windowOf().innerWidth}x${windowOf().innerHeight}`
        : `${fallback.width}x${fallback.height}`,
    () => `${fallback.width}x${fallback.height}`,
  );

  const [width, height] = packed.split("x");
  return { width: Number(width), height: Number(height) };
}
