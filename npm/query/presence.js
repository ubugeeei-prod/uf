// @flow
//
// `@uniflowed/query/presence`: the world moved while nobody was looking.
//
// Two facts sit outside every cache and invalidate all of it at once: the
// reader came back to the tab, and the network came back. Both mean the same
// thing — time passed during which this application could not have learned
// anything — and both are the reason a query library feels different from a
// `useEffect` that fetches once. A dashboard left open over lunch should not
// still be showing lunchtime.
//
// # Why the two live in one module
//
// They are the same five lines twice: a boolean, listeners, a browser event
// that sets it, and an override for the environments where the browser event
// does not exist. Splitting them would produce two files that differ only in
// the name of the event, and a reader who found one would have to be told
// about the other anyway.
//
// # Why the listeners are attached lazily and released
//
// A package may not do work while it is being imported: an application that
// imports `useQuery` and renders nothing must not have registered a
// `visibilitychange` handler, and a server that imports it has no `document`
// to register one on. So the handlers go on when the first watcher arrives and
// come off when the last one leaves, which also means a test that renders and
// unmounts leaves the process exactly as it found it.
//
// # Why `setFocused` and `setOnline` are public
//
// React Native has no `document` and no `window`; its equivalents are
// `AppState` and NetInfo, and an application there is expected to drive this
// object from them. Tests need the same door for the same reason. Providing it
// is what keeps the browser bindings from being the only way in — and what
// keeps this module honest about the fact that "focused" is an assumption, not
// an observation, wherever the platform does not report it.
//
// Nothing here pauses a request when the network is gone. A fetch that fails
// offline is a failure like any other, and [`../retry`] already knows what to
// do with one; a second mechanism that holds requests in a queue would have to
// agree with the first about ordering, cancellation and timeouts, and the
// disagreement is where the bugs would be.

/** Which of the two ambient facts changed. */
export type PresenceEvent = "focus" | "online";

export class Presence {
  focused: boolean;
  online: boolean;

  listeners: Set<(event: PresenceEvent) => void> = new Set();
  detach: (() => void) | null = null;

  constructor() {
    // Read once at construction rather than at import: the answer is the
    // environment's, and the environment does not exist yet while modules are
    // being evaluated on a server.
    const document = (globalThis as $FlowFixMe).document;
    const navigator = (globalThis as $FlowFixMe).navigator;
    this.focused = document == null || document.visibilityState !== "hidden";
    this.online = navigator == null || navigator.onLine !== false;
  }

  isFocused(): boolean {
    return this.focused;
  }

  isOnline(): boolean {
    return this.online;
  }

  /**
   * Report that the reader came back, or left.
   *
   * Only a change is announced. A platform that fires its focus event on every
   * window activation — including the one the reader never left — would
   * otherwise turn a click on the page into a refetch of everything.
   */
  setFocused(value: boolean): void {
    if (this.focused === value) {
      return;
    }
    this.focused = value;
    if (value) {
      this.announce("focus");
    }
  }

  /** Report that the network came back, or went away. */
  setOnline(value: boolean): void {
    if (this.online === value) {
      return;
    }
    this.online = value;
    if (value) {
      this.announce("online");
    }
  }

  /** Watch for either event. Returns the unsubscribe. */
  subscribe(listener: (event: PresenceEvent) => void): () => void {
    this.listeners.add(listener);
    if (this.listeners.size === 1) {
      this.attach();
    }
    return () => {
      this.listeners.delete(listener);
      if (this.listeners.size === 0) {
        this.release();
      }
    };
  }

  announce(event: PresenceEvent): void {
    for (const listener of Array.from(this.listeners)) {
      listener(event);
    }
  }

  attach(): void {
    const document = (globalThis as $FlowFixMe).document;
    const window = (globalThis as $FlowFixMe).window;
    if (document?.addEventListener == null && window?.addEventListener == null) {
      return;
    }

    const onVisibility = () => this.setFocused(document.visibilityState !== "hidden");
    const onOnline = () => this.setOnline(true);
    const onOffline = () => this.setOnline(false);

    document?.addEventListener?.("visibilitychange", onVisibility);
    window?.addEventListener?.("online", onOnline);
    window?.addEventListener?.("offline", onOffline);

    this.detach = () => {
      document?.removeEventListener?.("visibilitychange", onVisibility);
      window?.removeEventListener?.("online", onOnline);
      window?.removeEventListener?.("offline", onOffline);
    };
  }

  release(): void {
    this.detach?.();
    this.detach = null;
  }
}
