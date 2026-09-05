// @flow
//
// `@uniflowed/hooks/browser`: reading the environment, safely on a server.
//
// uf prerenders every static route, so each of these runs once where there is
// no `window`. `useSyncExternalStore` is what makes that correct rather than
// guarded: it takes a server snapshot as a separate argument, so the value
// used during prerender is stated rather than being whatever a `typeof window`
// check happened to fall through to. It also means React reads the value at
// the moment it commits, which is what stops a media query changing between
// render and paint from tearing.
//
// # What belongs in this module
//
// A reading of the one browser the page is in: its size, its scroll offset,
// its connection, its visibility, its position on the earth, the preferences
// the reader set, the permissions the reader granted. There is exactly one
// answer at a time, nobody has to pass anything in to ask, and a server has no
// answer at all — which is why every hook here either takes a server value
// from the caller or states an honest default.
//
// Not here: anything about a specific element, which needs a ref and lives in
// `dom.js`. `useDocumentVisible` is the closest call in the package and stays
// here, because the document is the environment rather than a node a caller
// chose.
//
// # The one that writes
//
// `useScrollLock` is the exception to "reading", and it is here rather than in
// `dom.js` because what it freezes is the page: there is one of it, the caller
// has no ref to it, and the compensation it has to make — the width of the
// scrollbar that is about to disappear — is a fact about the window rather
// than about any element. A lock that took a ref would be a different and
// rarer hook.
//
// # Two kinds of hook, and why the second kind exists
//
// Most of these are a `useSyncExternalStore` over an event the browser already
// fires, which is the shape that survives a prerender and a concurrent render
// without tearing. Three are not: `useGeolocation` and `usePermission` are
// subscriptions whose *first* value only arrives asynchronously, so there is
// nothing for a snapshot to return until it does, and `useScrollLock` writes.
// Each says so where it is defined.

import { useCallback, useEffect, useMemo, useState, useSyncExternalStore } from "@uniflowed/react";

import { useIsomorphicLayoutEffect } from "./lifecycle.js";

/**
 * The part of a `navigator` this package reads.
 *
 * Not a declaration of `Navigator` — Flow ships one of those. This is the
 * list of what these hooks actually touch, which is short enough to be worth
 * writing down and is the reason none of them needs an `any`: everything below
 * `browserWindow()` is checked against this.
 *
 * Every field is optional because a hosted document is not required to carry
 * the whole of a browser. `navigator.geolocation` is `null` under happy-dom
 * and absent under a bare Node global, and `navigator.connection` exists
 * nowhere but Chromium.
 */
export type BrowserNavigator = {
  readonly onLine?: boolean,
  readonly userAgent?: string,
  readonly clipboard?: ?{
    readonly readText: () => Promise<string>,
    readonly writeText: (text: string) => Promise<void>,
    ...
  },
  readonly geolocation?: ?Geolocation,
  readonly permissions?: ?{
    readonly query: (descriptor: { readonly name: string, ... }) => Promise<PermissionStatus>,
    ...
  },
  readonly connection?: ?NetworkConnection,
  ...
};

/** The Network Information object, which only Chromium has. */
export type NetworkConnection = {
  readonly downlink?: number,
  readonly effectiveType?: string,
  readonly saveData?: boolean,
  readonly addEventListener?: (type: string, listener: () => mixed) => void,
  readonly removeEventListener?: (type: string, listener: () => mixed) => void,
  ...
};

/**
 * The part of a `window` this package reads.
 *
 * Same idea as [`BrowserNavigator`], and the same reason: naming what is
 * touched turns every read in the package into a checked one. The observer
 * constructors are optional because a browser old enough to lack one is a
 * browser a hook here has to keep working in — it degrades to reporting
 * nothing rather than throwing during a render.
 */
export type BrowserWindow = {
  readonly document: Document,
  readonly navigator: BrowserNavigator,
  readonly localStorage?: ?Storage,
  readonly sessionStorage?: ?Storage,
  readonly innerWidth: number,
  readonly innerHeight: number,
  readonly scrollX: number,
  readonly scrollY: number,
  readonly matchMedia?: (query: string) => MediaQueryList,
  readonly getComputedStyle?: (element: Element) => CSSStyleDeclaration,
  readonly requestAnimationFrame?: (callback: (time: number) => mixed) => AnimationFrameID,
  readonly cancelAnimationFrame?: (handle: AnimationFrameID) => void,
  // Two overloads, the way Flow's own `EventTarget` is declared: a `storage`
  // listener is handed a `StorageEvent` and needs its `key`, and narrowing an
  // `Event` down to one at runtime would mean an `instanceof StorageEvent`
  // against a name that is not defined in every host a uf test runs in.
  readonly addEventListener: ((
    type: "storage",
    listener: (event: StorageEvent) => mixed,
    options?: EventListenerOptionsOrUseCapture,
  ) => void) &
    ((
      type: string,
      listener: (event: Event) => mixed,
      options?: EventListenerOptionsOrUseCapture,
    ) => void),
  readonly removeEventListener: ((
    type: "storage",
    listener: (event: StorageEvent) => mixed,
    options?: EventListenerOptionsOrUseCapture,
  ) => void) &
    ((
      type: string,
      listener: (event: Event) => mixed,
      options?: EventListenerOptionsOrUseCapture,
    ) => void),
  readonly ResizeObserver?: Class<ResizeObserver>,
  readonly IntersectionObserver?: Class<IntersectionObserver>,
  readonly MutationObserver?: Class<MutationObserver>,
  ...
};

/**
 * The window these hooks listen to, or `null` where there is no browser.
 *
 * In a browser `globalThis` *is* the window, so `globalThis.addEventListener`
 * looks correct. It is not correct anywhere a document has been installed onto
 * another host's global — which is every uf test process, where `globalThis` is
 * Node's and has no `addEventListener` at all. Ask the document's own window
 * for its methods and both cases work.
 *
 * Exported because it is the first question every hook in this package asks,
 * and an application writing its own prerender-safe hook has to ask it too.
 * The single `?? globalThis` is this package's only unchecked step: `window` is
 * `any` in Flow's own library definition, and this is where that stops.
 */
export function browserWindow(): BrowserWindow | null {
  if (typeof globalThis.document === "undefined") {
    return null;
  }
  return globalThis.window ?? globalThis;
}

/** A subscription to nothing, for a value that cannot change. */
function subscribeToNothing(): () => void {
  return () => {};
}

/** The server's answer to every "is this available" question. */
function unsupported(): boolean {
  return false;
}

/**
 * Whether a capability the browser may not have is there.
 *
 * The naive version — `typeof window.BroadcastChannel === "function"` in the
 * render — is a hydration mismatch waiting to happen: the server says one
 * thing, the client's first render says another, and React reports it against
 * whatever markup happened to differ. Asked through `useSyncExternalStore`, the
 * server's answer is `false`, the hydrating render agrees with it, and React
 * re-renders with the truth immediately afterwards.
 *
 * `probe` is called during render, so it must only look — never install, never
 * request. It is passed straight through rather than stabilised: a snapshot is
 * a question the render is asking now, and a stable callback's body is
 * installed in an insertion effect that has not run yet, so it would answer
 * from the render before.
 */
export hook useSupported(probe: () => boolean): boolean {
  return useSyncExternalStore(subscribeToNothing, probe, unsupported);
}

/**
 * Whether a media query matches.
 *
 * `serverValue` is what a prerender should assume, and it has no honest
 * default — a page that hides a sidebar under 48rem wants `false` on the
 * server, and one that renders a mobile menu wants `true`. So the caller says.
 */
export hook useMediaQuery(query: string, serverValue: boolean = false): boolean {
  const subscribe = useCallback(
    (notify: () => void) => {
      const list = browserWindow()?.matchMedia?.(query);
      if (list == null) {
        return () => {};
      }
      list.addEventListener("change", notify);
      return () => list.removeEventListener("change", notify);
    },
    [query],
  );

  const snapshot = useCallback(
    () => browserWindow()?.matchMedia?.(query)?.matches ?? serverValue,
    [query, serverValue],
  );

  return useSyncExternalStore(subscribe, snapshot, () => serverValue);
}

/** The reader's colour-scheme preference. */
export hook usePreferredColorScheme(serverValue: "light" | "dark" = "light"): "light" | "dark" {
  return useMediaQuery("(prefers-color-scheme: dark)", serverValue === "dark") ? "dark" : "light";
}

/** Whether the reader has asked for less motion. */
export hook usePrefersReducedMotion(serverValue: boolean = false): boolean {
  return useMediaQuery("(prefers-reduced-motion: reduce)", serverValue);
}

/** Whether the browser thinks it is online. */
export hook useOnline(serverValue: boolean = true): boolean {
  const subscribe = useCallback((notify: () => void) => {
    const win = browserWindow();
    if (win == null) {
      return () => {};
    }
    win.addEventListener("online", notify);
    win.addEventListener("offline", notify);
    return () => {
      win.removeEventListener("online", notify);
      win.removeEventListener("offline", notify);
    };
  }, []);

  const snapshot = useCallback(
    () => browserWindow()?.navigator.onLine ?? serverValue,
    [serverValue],
  );

  return useSyncExternalStore(subscribe, snapshot, () => serverValue);
}

/** Whether the document is the one the reader is looking at. */
export hook useDocumentVisible(serverValue: boolean = true): boolean {
  const subscribe = useCallback((notify: () => void) => {
    const document = browserWindow()?.document;
    if (document == null) {
      return () => {};
    }
    document.addEventListener("visibilitychange", notify);
    return () => document.removeEventListener("visibilitychange", notify);
  }, []);

  const snapshot = useCallback(() => {
    const document = browserWindow()?.document;
    return document == null ? serverValue : document.visibilityState !== "hidden";
  }, [serverValue]);

  return useSyncExternalStore(subscribe, snapshot, () => serverValue);
}

/**
 * How far something has been scrolled.
 *
 * Defined here rather than in `dom.js` because `dom.js` imports this module
 * and not the other way round; `useScroll` over an element uses the same
 * shape, and one name for one thing is worth the arrow.
 */
export type ScrollOffset = {| readonly x: number, readonly y: number |};

/** How big something is. Shared with `useElementSize` for the same reason. */
export type Size = {| readonly width: number, readonly height: number |};

/** Read `"12x34"` back into a pair. */
function unpack(packed: string): ScrollOffset {
  const [x, y] = packed.split("x");
  return { x: Number(x), y: Number(y) };
}

/** The size of the viewport. */
export hook useWindowSize(serverValue?: Size): Size {
  const width = serverValue?.width ?? 0;
  const height = serverValue?.height ?? 0;

  const subscribe = useCallback((notify: () => void) => {
    const win = browserWindow();
    if (win == null) {
      return () => {};
    }
    win.addEventListener("resize", notify);
    return () => win.removeEventListener("resize", notify);
  }, []);

  // A string snapshot, because `useSyncExternalStore` compares snapshots by
  // identity: returning a fresh object every time would re-render on every
  // check, which is an infinite loop React reports rather than tolerates.
  const packed = useSyncExternalStore(
    subscribe,
    useCallback(() => {
      const win = browserWindow();
      return win == null ? `${width}x${height}` : `${win.innerWidth}x${win.innerHeight}`;
    }, [width, height]),
    useCallback(() => `${width}x${height}`, [width, height]),
  );

  const size = unpack(packed);
  return { width: size.x, height: size.y };
}

/**
 * How far the page has been scrolled.
 *
 * The same packed-string snapshot as `useWindowSize`, for the same reason, and
 * a passive listener because a scroll handler that could call
 * `preventDefault` blocks scrolling on a touch screen until it has run.
 */
export hook useWindowScroll(serverValue?: ScrollOffset): ScrollOffset {
  const x = serverValue?.x ?? 0;
  const y = serverValue?.y ?? 0;

  const subscribe = useCallback((notify: () => void) => {
    const win = browserWindow();
    if (win == null) {
      return () => {};
    }
    win.addEventListener("scroll", notify, { passive: true });
    return () => win.removeEventListener("scroll", notify);
  }, []);

  const packed = useSyncExternalStore(
    subscribe,
    useCallback(() => {
      const win = browserWindow();
      return win == null ? `${x}x${y}` : `${win.scrollX}x${win.scrollY}`;
    }, [x, y]),
    useCallback(() => `${x}x${y}`, [x, y]),
  );

  return unpack(packed);
}

/**
 * How many components are currently holding the page still.
 *
 * Module-level rather than per-component, because two dialogs open at once
 * must not have the first one to close put the page back: the page unlocks
 * when the last of them lets go. Strict Mode's mount-unmount-mount is
 * balanced by construction — the effect increments and its cleanup decrements.
 */
let scrollLocks = 0;

/** What to put back when the last lock is released. */
let releaseScroll: (() => void) | null = null;

/**
 * The widest a scrollbar is allowed to be believed.
 *
 * The compensation is `innerWidth - documentElement.clientWidth`, which is the
 * scrollbar's width in a browser that lays out and is the whole viewport in
 * one that does not — a headless document reports a client width of zero.
 * Padding the page by a viewport pushes it off screen, so a number that could
 * not be a scrollbar is treated as no measurement at all.
 */
const WIDEST_SCROLLBAR = 40;

function lockScroll(): void {
  scrollLocks += 1;
  if (scrollLocks > 1) {
    return;
  }
  const win = browserWindow();
  const body = win?.document.body;
  if (win == null || body == null) {
    return;
  }
  const previousOverflow = body.style.overflow;
  const previousPadding = body.style.paddingRight;
  const root = win.document.documentElement;
  const gap = root == null ? 0 : win.innerWidth - root.clientWidth;
  body.style.overflow = "hidden";
  if (gap > 0 && gap <= WIDEST_SCROLLBAR) {
    const computed = win.getComputedStyle?.(body).paddingRight ?? "";
    body.style.paddingRight = `${(Number.parseFloat(computed) || 0) + gap}px`;
  }
  releaseScroll = () => {
    body.style.overflow = previousOverflow;
    body.style.paddingRight = previousPadding;
  };
}

function unlockScroll(): void {
  scrollLocks = Math.max(0, scrollLocks - 1);
  if (scrollLocks === 0 && releaseScroll != null) {
    releaseScroll();
    releaseScroll = null;
  }
}

/**
 * Hold the page still while `locked`.
 *
 * A layout effect, so the page is frozen before the frame in which the dialog
 * that asked for it appears — an ordinary effect lets one frame of scrolling
 * through, which reads as a jump.
 *
 * On a server this does nothing at all: effects do not run during a prerender,
 * so a locked dialog rendered into HTML leaves the markup alone.
 */
export hook useScrollLock(locked: boolean): void {
  useIsomorphicLayoutEffect(() => {
    if (!locked) {
      return;
    }
    lockScroll();
    return unlockScroll;
  }, [locked]);
}

/** How good the connection is, as the browser grades it. */
export type EffectiveConnectionType = "slow-2g" | "2g" | "3g" | "4g";

/** What the browser will say about the connection. */
export type Network = {|
  readonly online: boolean,
  /** Estimated bandwidth in megabits per second, where the browser reports it. */
  readonly downlink: number | null,
  readonly effectiveType: EffectiveConnectionType | null,
  /** Whether the reader has asked for less data to be used. */
  readonly saveData: boolean,
  /** Whether anything beyond `online` was actually measured. */
  readonly supported: boolean,
|};

const EFFECTIVE_TYPES: $ReadOnlyArray<EffectiveConnectionType> = ["slow-2g", "2g", "3g", "4g"];

function asEffectiveType(value: string): EffectiveConnectionType | null {
  for (const known of EFFECTIVE_TYPES) {
    if (known === value) {
      return known;
    }
  }
  return null;
}

/**
 * What the browser will say about the connection.
 *
 * Only Chromium implements Network Information, so `supported` is part of the
 * answer rather than something a caller has to find out by seeing nulls. The
 * `online` half is everywhere, and stays true even where the rest is unknown —
 * which is the field almost every caller wants.
 *
 * The snapshot is a packed string for the reason `useWindowSize` gives: an
 * object rebuilt on every check never compares equal, and `useSyncExternalStore`
 * would re-render forever.
 */
export hook useNetwork(serverValue: boolean = true): Network {
  const subscribe = useCallback((notify: () => void) => {
    const win = browserWindow();
    if (win == null) {
      return () => {};
    }
    const connection = win.navigator.connection;
    win.addEventListener("online", notify);
    win.addEventListener("offline", notify);
    connection?.addEventListener?.("change", notify);
    return () => {
      win.removeEventListener("online", notify);
      win.removeEventListener("offline", notify);
      connection?.removeEventListener?.("change", notify);
    };
  }, []);

  const server = useCallback(() => `${serverValue ? "1" : "0"}|||0`, [serverValue]);

  const packed = useSyncExternalStore(
    subscribe,
    useCallback(() => {
      const navigator = browserWindow()?.navigator;
      if (navigator == null) {
        return `${serverValue ? "1" : "0"}|||0`;
      }
      const connection = navigator.connection;
      const online = navigator.onLine ?? serverValue;
      if (connection == null) {
        return `${online ? "1" : "0"}|||0`;
      }
      const downlink = connection.downlink;
      return [
        online ? "1" : "0",
        downlink == null ? "" : String(downlink),
        connection.effectiveType ?? "",
        connection.saveData === true ? "1" : "0",
      ].join("|");
    }, [serverValue]),
    server,
  );

  return useMemo(() => {
    const [online, downlink, effectiveType, saveData] = packed.split("|");
    return {
      online: online === "1",
      downlink: downlink === "" ? null : Number(downlink),
      effectiveType: asEffectiveType(effectiveType),
      saveData: saveData === "1",
      supported: downlink !== "" || effectiveType !== "",
    };
  }, [packed]);
}

/** Where the reader is, to the accuracy the browser was willing to give. */
export type Geoposition = {|
  readonly latitude: number,
  readonly longitude: number,
  /** Radius of a 95% confidence circle, in metres. */
  readonly accuracy: number,
  readonly timestamp: number,
|};

/** What `useGeolocation` knows so far. */
export type GeolocationReading = {|
  readonly position: Geoposition | null,
  readonly error: Error | null,
  readonly supported: boolean,
|};

/**
 * Watch where the reader is.
 *
 * An effect rather than a `useSyncExternalStore`, and the difference is not
 * stylistic: there is no snapshot to read. The browser has no "current
 * position" property to ask — the first value arrives in a callback, after a
 * permission prompt the reader may take a minute to answer or never answer at
 * all. So `position` is `null` until one arrives, on a server and in a browser
 * alike, which is also what makes it hydration-safe.
 *
 * Mounting this asks the reader for permission. Mount it on the page that
 * needs a position, not at the top of an application.
 */
export hook useGeolocation(options?: {|
  readonly enabled?: boolean,
  readonly highAccuracy?: boolean,
  readonly maximumAge?: number,
  readonly timeout?: number,
|}): GeolocationReading {
  const enabled = options?.enabled ?? true;
  const highAccuracy = options?.highAccuracy ?? false;
  const maximumAge = options?.maximumAge;
  const timeout = options?.timeout;

  const supported = useSupported(() => browserWindow()?.navigator.geolocation != null);
  const [reading, setReading] = useState<{|
    position: Geoposition | null,
    error: Error | null,
  |}>({ position: null, error: null });

  useEffect(() => {
    const geolocation = browserWindow()?.navigator.geolocation;
    if (!enabled || geolocation == null) {
      return;
    }
    const watch = geolocation.watchPosition(
      (position: Position) => {
        setReading({
          position: {
            latitude: position.coords.latitude,
            longitude: position.coords.longitude,
            accuracy: position.coords.accuracy,
            timestamp: position.timestamp,
          },
          error: null,
        });
      },
      (failure: PositionError) => {
        // The position already on screen is kept: a timeout on the third
        // reading does not mean the second one stopped being true.
        setReading((current) => ({ ...current, error: new Error(failure.message) }));
      },
      { enableHighAccuracy: highAccuracy, maximumAge, timeout },
    );
    return () => geolocation.clearWatch(watch);
  }, [enabled, highAccuracy, maximumAge, timeout]);

  return useMemo(
    () => ({ position: reading.position, error: reading.error, supported }),
    [reading, supported],
  );
}

/** A permission this hook knows how to ask about. */
export type PermissionName =
  | "geolocation"
  | "notifications"
  | "camera"
  | "microphone"
  | "clipboard-read"
  | "clipboard-write"
  | "persistent-storage"
  | "push"
  | "midi";

/**
 * What the browser says about a permission.
 *
 * `"unknown"` is one value for four situations that a caller treats the same
 * way — no Permissions API, a name this browser does not recognise, an answer
 * that has not arrived yet, and a server render. Splitting them would make
 * every caller write the same four-armed `match` to reach the same conclusion.
 */
export type PermissionAnswer = "granted" | "denied" | "prompt" | "unknown";

/**
 * Whether the reader has granted a permission, without asking for it.
 *
 * Querying is not prompting: this reports the current state and follows it if
 * the reader changes their mind in browser settings. Asking for the permission
 * is the API's own job — `getUserMedia`, `watchPosition` — and doing it from
 * here would make a hook that reads have a side effect nobody asked for.
 *
 * An effect rather than a store, for the reason `useGeolocation` gives: the
 * answer is a promise, so there is nothing to read synchronously.
 */
export hook usePermission(name: PermissionName): PermissionAnswer {
  const [answer, setAnswer] = useState<PermissionAnswer>("unknown");

  useEffect(() => {
    const permissions = browserWindow()?.navigator.permissions;
    if (permissions == null) {
      return;
    }
    // Set when the effect is superseded, so an answer that arrives for a name
    // the caller has stopped asking about is dropped rather than shown.
    let ignore = false;
    let status: PermissionStatus | null = null;
    const onChange = () => {
      if (!ignore && status != null) {
        setAnswer(status.state);
      }
    };

    permissions.query({ name }).then(
      (result: PermissionStatus) => {
        if (ignore) {
          return;
        }
        status = result;
        setAnswer(result.state);
        result.addEventListener("change", onChange);
      },
      () => {
        // A name this browser does not know rejects rather than answering.
        if (!ignore) {
          setAnswer("unknown");
        }
      },
    );

    return () => {
      ignore = true;
      status?.removeEventListener("change", onChange);
    };
  }, [name]);

  return answer;
}
