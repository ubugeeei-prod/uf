// @flow
//
// `@uniflowed/hooks/channels`: a value that came from outside this page.
//
// Two hooks, and the thing they have in common is the reason they are not in
// `state.js`. A piece of state is a value the component owns and can always
// read. What is here is a value that *arrives*: a message another tab sent, a
// string the reader copied out of a different application. Between arrivals
// there is nothing to read, the browser may refuse the whole facility, and
// every operation is asynchronous or fire-and-forget.
//
// `useStorage` is the one to compare them with, and the line is durability
// rather than direction. `localStorage` holds a value: it is there on the next
// visit, two components reading the same key must agree about it, and
// `useSyncExternalStore` is the right shape because there is always a snapshot.
// A broadcast happens once and is gone; a clipboard belongs to the operating
// system and can change without telling the page. Neither has a snapshot, so
// neither is a store.
//
// # What belongs in this module
//
// A hook whose subject is the page's boundary — something crossing into or out
// of this document that is not the server. Not the server: a request is
// `@uniflowed/query`'s and `@uniflowed/fetch`'s, and the distinction is worth
// keeping because those come with caching, retry and revalidation that none of
// this wants.
//
// # Before hydration
//
// Both hooks report `supported: false` on a server *and* on the client's first
// render, through `useSupported` — see the reason there. Neither opens
// anything during a render: the channel is created in an effect, and the
// clipboard is only ever touched from a callback the reader caused.

import { useCallback, useEffect, useMemo, useRef, useState } from "@uniflowed/react";

import { browserWindow, useSupported } from "./browser.js";
import { useStableCallback } from "./lifecycle.js";

/**
 * The part of `BroadcastChannel` this module uses.
 *
 * Declared here rather than taken from Flow's library definition for two
 * reasons: the constructor is looked up at runtime and may be absent, so it
 * has to be a value with a known constructor signature; and the listener is
 * typed with the one field that is read, which is what removes the
 * `instanceof MessageEvent` a narrowing would otherwise need — a name that is
 * not defined in every host these tests run in.
 */
declare class Channel {
  constructor(name: string): void;
  postMessage(message: mixed): void;
  close(): void;
  addEventListener(type: "message", listener: (event: { data: mixed, ... }) => mixed): void;
  removeEventListener(type: "message", listener: (event: { data: mixed, ... }) => mixed): void;
}

/**
 * `BroadcastChannel`, which is the host's rather than the document's.
 *
 * In a browser the two are the same object. In a process where a document has
 * been installed onto another runtime's global they are not, and the host is
 * the one that has this: a broadcast channel is a facility of the runtime, and
 * a hosted `Window` is not required to carry one. That is also why this is not
 * a field on `BrowserWindow` — it is not read from the window.
 */
declare var BroadcastChannel: Class<Channel> | void;

function channelConstructor(): Class<Channel> | null {
  return typeof BroadcastChannel === "undefined" ? null : (BroadcastChannel ?? null);
}

/** What `useBroadcast` hands back. */
export type UseBroadcastReturn<T> = {|
  /** Send to every other page on this origin. Never to this one. */
  readonly post: (message: T) => void,
  readonly supported: boolean,
|};

/**
 * Send and receive messages between the tabs of one origin.
 *
 * A callback rather than a `latest` value, and that is the design decision in
 * this file. A message is an event: it happened, it was worth acting on, and
 * holding the most recent one as state invites a component to treat it as the
 * truth — which it is not, because the tab that just opened has never received
 * one and has no way to ask. Where two tabs need to agree about a *value*,
 * `useStorage` is the hook: it has a snapshot every tab can read, and it
 * already notifies across tabs.
 *
 * A channel does not deliver to the page that posted. Two components in one
 * tab sharing a channel therefore do not hear each other, which is the
 * specification's behaviour and not this hook's.
 *
 * `T` is what the caller promises to send. Nothing checks what arrives — it
 * was serialised by another copy of the page, possibly an older deployment of
 * it — so `onMessage` is handed `mixed` and validating it is the caller's, with
 * `@uniflowed/validator` if it matters.
 */
export hook useBroadcast<T>(
  name: string,
  onMessage: (message: mixed) => mixed,
): UseBroadcastReturn<T> {
  const stable = useStableCallback(onMessage);
  const supported = useSupported(() => channelConstructor() != null);

  // Written in an effect and read only from `post`, which a render never
  // calls. A render that React throws away cannot see it, and a render that
  // commits does not need to.
  const channel = useRef<Channel | null>(null);

  useEffect(() => {
    const Constructor = channelConstructor();
    if (Constructor == null) {
      return;
    }
    const open = new Constructor(name);
    channel.current = open;
    const listener = (event: { data: mixed, ... }) => {
      stable(event.data);
    };
    open.addEventListener("message", listener);
    return () => {
      open.removeEventListener("message", listener);
      open.close();
      // Safe unconditionally: React runs an effect's cleanup before the effect
      // that replaces it, so this can never clear a channel a later mount has
      // already installed — including Strict Mode's second mount.
      channel.current = null;
    };
  }, [name, stable]);

  const post = useStableCallback((message: T) => {
    channel.current?.postMessage(message);
  });

  return useMemo(() => ({ post, supported }), [post, supported]);
}

/** What `useClipboard` hands back. */
export type UseClipboardReturn = {|
  /** Put text on the clipboard. Resolves to whether it worked. */
  readonly copy: (text: string) => Promise<boolean>,
  /** Read the clipboard, which may prompt. `null` when it is refused. */
  readonly read: () => Promise<string | null>,
  /** True for `resetAfter` milliseconds following a successful copy. */
  readonly copied: boolean,
  readonly supported: boolean,
|};

/**
 * The system clipboard.
 *
 * `copied` exists because every copy button needs it: a tick that appears for
 * a moment is the only feedback a copy has. It resets itself, and the timer is
 * cleared at unmount, so a component that goes away during the pause does not
 * set state afterwards.
 *
 * Reading is a function rather than a value, deliberately. There is no event
 * for "the clipboard changed", so a hook that exposed its contents as state
 * would have to poll — and polling the clipboard means asking for permission
 * repeatedly and reading whatever the reader copied out of their password
 * manager. `read()` is called when the reader asks for a paste, which is when
 * the browser expects to be asked.
 *
 * `document.execCommand("copy")` is not used as a fallback. It is deprecated,
 * it requires a hidden element and a selection, and the browsers that lack the
 * asynchronous clipboard in 2026 are the ones that would not run this package
 * anyway. `supported` says so instead of pretending.
 */
export hook useClipboard(options?: {| readonly resetAfter?: number |}): UseClipboardReturn {
  const resetAfter = options?.resetAfter ?? 1_500;
  const supported = useSupported(() => browserWindow()?.navigator.clipboard != null);
  const [copied, setCopied] = useState(false);

  const timer = useRef<TimeoutID | null>(null);
  useEffect(
    () => () => {
      if (timer.current != null) {
        clearTimeout(timer.current);
      }
    },
    [],
  );

  const copy = useCallback(
    async (text: string): Promise<boolean> => {
      const clipboard = browserWindow()?.navigator.clipboard;
      if (clipboard == null) {
        return false;
      }
      try {
        await clipboard.writeText(text);
      } catch {
        // Refused, or the document was not focused when the write landed.
        return false;
      }
      setCopied(true);
      if (timer.current != null) {
        clearTimeout(timer.current);
      }
      timer.current = setTimeout(() => {
        timer.current = null;
        setCopied(false);
      }, resetAfter);
      return true;
    },
    [resetAfter],
  );

  const read = useCallback(async (): Promise<string | null> => {
    const clipboard = browserWindow()?.navigator.clipboard;
    if (clipboard == null) {
      return null;
    }
    try {
      return await clipboard.readText();
    } catch {
      return null;
    }
  }, []);

  return useMemo(() => ({ copy, read, copied, supported }), [copy, read, copied, supported]);
}
