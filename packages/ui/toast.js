// @flow
//
// Notifications, and the live region that was already watching.
//
// `combobox.js` states the rule this whole component exists for:
//
// > A live region added to the page in the same commit as the text it holds is
// > usually not announced, because the technology watching it had nothing to
// > watch until it was already too late; leaving it mounted and empty is what
// > makes the *next* change speak.
//
// Every hand-written toast mounts a `<div role="alert">` at the moment the
// message arrives, which is exactly the commit that makes it silent. It looks
// perfect on screen, it passes every review, and the reader who most needs to
// be told that the save failed is told nothing. That is the bug, it is
// invisible, and one component in the layout is the only place it can be fixed
// once for a whole application.
//
// So `Toast.Region` renders on mount and stays, holding nothing. Notifications
// are appended into it.
//
// # Two regions, not one
//
// "Saved" and "could not save" interrupt a reader differently and should, and
// which of the two a notification is belongs to the notification rather than
// to the application. That cannot be a role that changes: swapping a live
// region's `role` or `aria-live` while it is being watched is the same bug as
// mounting it late, because the technology is watching the region it saw. So
// there are two regions, both mounted from the start and both empty —
// `role="status"` and `role="alert"` — and a notification is appended into the
// one that matches it.
//
// What that costs is chronological order between the two piles: a failure and
// a success on screen together are in separate containers, and no CSS
// interleaves them. It is written down rather than discovered, and it buys the
// thing that matters more, which is that what a reader hears is exactly what a
// reader sees. The alternative — one visible stack plus hidden announcers
// mirroring its text — puts every notification in the document twice, so a
// reader browsing the page finds each one again with no way to tell it is the
// same one.
//
// # `aria-atomic`, and why `limit` is an accessibility setting
//
// Both regions are `aria-atomic="true"`, so a change presents the whole
// region. With one notification showing — the ordinary case — that is exactly
// right, and it is what makes a two-line notification read as one sentence
// rather than as a fragment. With three showing, adding a fourth reads all
// four. `limit` is therefore not only how tall the stack is allowed to get; it
// is how much a reader is made to listen to. Three is the default because it
// is about as much as anyone will hear out.
//
// # Focus is never taken, and there is a key that gives it
//
// Nothing here calls `focus()` when a notification appears. Moving focus to a
// toast interrupts whatever the reader was typing, and it is the single
// failure that makes people turn notifications off.
//
// But a notification carrying an Undo button that vanishes after four seconds
// is a control no keyboard reader can operate, so the region is a named
// landmark — `role="region"` with an `aria-label` — and `F6`, the key the APG
// suggests for moving between panes, moves focus into it. It moves focus to
// the region itself rather than to the first button in it, which is
// deliberate: landing on the region is what makes a screen reader read the
// region's name and its contents, and landing on a button skips straight past
// both. `Tab` from there reaches the buttons in order. Pressing `F6` again
// while focus is inside gives it back to wherever it came from, because a key
// that only goes one way strands the reader it was meant to help.
//
// `F6` is bound while the reader is typing, which is the one case that matters:
// a notification that arrives during a form fill is the notification most
// likely to be about the form.
//
// # Timers that stop
//
// WCAG 2.2.1, *Timing Adjustable*, applies to anything that disappears on its
// own. The countdown stops while the pointer is over a notification, while
// focus is inside it, and while the document is hidden — the third because a
// reader who switches tabs for a minute should not come back to an empty
// region and no idea what they missed.
//
// It *stops*; it does not restart. The one-line version of this is
// `useTimeout(dismiss, paused ? null : duration)`, which reads correctly and
// is wrong: `useTimeout` re-arms whenever its delay changes, so every unpause
// gives the notification its whole life again, and a reader who brushes the
// stack with the pointer keeps it on screen indefinitely. What is left is
// banked instead, which costs a clock and two refs and is what "pause" means.
//
// A notification given `duration: null` never expires at all, which is what
// anything carrying an action should be.
//
// # The queue lives outside React
//
// `toast("Saved")` is called from an event handler, from a `catch`, from a
// Server Action's error path — none of which have a component to put state in.
// So the queue is a store in this module, read through `useSyncExternalStore`,
// which is what `ubugeeei-redundancy.md` requires of an external store: cached
// immutable snapshots, and a server snapshot consistent with them. Nothing here
// is a mutable array a render reads.
//
// Three properties are what that hook is actually asking for, and the store
// below has those three and nothing more:
//
// - *An immutable snapshot whose reference changes only on a write.* The
//   getter hands back the array it is holding rather than building one, and a
//   write that computes the value already there is dropped rather than
//   announced. A getter that returns a fresh `[]` is a new identity every time
//   React asks, which renders, which asks again — the infinite loop React
//   reports rather than tolerates.
// - *A server snapshot consistent with the first client render.* It is the
//   same getter, so both sides see the same frozen `NONE` and hydration
//   compares like with like instead of against a placeholder.
// - *Module scope.* `toast()` is a function rather than a hook, so it has no
//   component and no context to reach state through — and an event handler, a
//   `catch` and a Server Action's error path all have to be able to call it.
//
// The third of those settles the scoping question as well: one page, one set
// of notifications. A queue something could scope to a subtree would leave a
// region showing an empty stack while `toast()` filled up a store it had no
// way to find.
//
// # Why this is not an atom in `@uniflowed/state`
//
// It was one, and it should be one again. A queue read through
// `useSyncExternalStore` is exactly what an atom is for, `@uniflowed/state` is
// what this project offers instead of Jotai, and the version of this file that
// imported `atom`, `read`, `subscribe` and `write` was not making a mistake.
//
// It cannot ship that way today. `@uniflowed/ui` is published to npm and
// `@uniflowed/state` is not: its name has never been bound, binding it takes a
// person with an `npm login` session and a 2FA prompt, and that is
// ubugeeei-prod/uf#210. Until it happens the package waits in
// `tools/release/pending-packages.txt`. A published package whose dependency
// is missing installs as nothing — `ETARGET` on the first thing a user types —
// so `tools/release/verify-npm.sh` refuses to release `@uniflowed/ui` while it
// declares that dependency, and it is right to refuse.
//
// So the store below is the shippable design rather than the better one. It is
// a second implementation of something this repository already has, written
// out by hand because the first one cannot be installed. When #210 binds the
// name and `state` moves into `published-packages.txt`, this goes back to an
// atom and this section goes with it.
//
// This is the first `useSyncExternalStore` in this package, and `index.js` says
// the package deliberately does not use one. That sentence is about reading
// *layout* during a render, which is what the DOM-measuring components here
// avoid by writing what they measured into state from an effect. A queue that
// lives outside React is the case the API is for, and `index.js` now says both
// things.

"use client";

import * as React from "@uniflowed/react";
import {
  createContext,
  useContext,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "@uniflowed/react";
import { useDocumentVisible } from "@uniflowed/hooks/browser";
import { useElementRef, useFocusWithin, useHover } from "@uniflowed/hooks/dom";
import { useKeyCombo } from "@uniflowed/hooks/keyboard";
import { useTimeout } from "@uniflowed/hooks/timing";

import type { Rest } from "./internal/merge-props.js";
import { composeHandlers, composeRefs, withoutComposed } from "./internal/merge-props.js";

/** How loudly a notification interrupts. */
export type Urgency = "polite" | "assertive";

/** One notification in the queue. */
export type Notification = {|
  readonly id: string,
  /** What a reader is told. A node, so a caller may render their own markup. */
  readonly content: React.Node,
  readonly urgency: Urgency,
  /** How long it stays, in milliseconds, or null for one that never expires. */
  readonly duration: number | null,
|};

/** What `toast` accepts beside the message. */
export type ToastOptions = {|
  readonly urgency?: Urgency,
  readonly duration?: number | null,
|};

/** What `updateToast` may change about a notification already queued. */
export type ToastChanges = {|
  readonly content?: React.Node,
  readonly urgency?: Urgency,
  readonly duration?: number | null,
|};

/**
 * Long enough to read a sentence, short enough not to be in the way.
 *
 * Anything carrying an action should pass `duration: null` instead of a longer
 * number: "long enough to notice, decide and reach the button" is not a
 * duration anybody can guess for somebody else.
 */
const DEFAULT_DURATION = 5000;

/**
 * The empty queue, as one value.
 *
 * A fresh `[]` from the snapshot getter is a new identity every time React
 * asks, which `useSyncExternalStore` reads as a change, which renders, which
 * asks again — the infinite loop React reports rather than tolerates. One
 * frozen constant is also what makes the server snapshot consistent with the
 * client's first one.
 */
const NONE: $ReadOnlyArray<Notification> = Object.freeze([]);

/**
 * The queue, and what is watching it.
 *
 * Module scope is the requirement rather than a convenience: `toast()` has
 * nowhere else to put this. It costs a server nothing — importing this file
 * allocates one frozen array and one empty `Set` and starts no work, and
 * nothing on a server calls `toast()`, because this module is `"use client"`.
 */
let queue: $ReadOnlyArray<Notification> = NONE;
const watchers: Set<() => void> = new Set();

/**
 * Replace the queue, and wake what is watching it.
 *
 * `change` is handed the current queue and returns the next one. Returning the
 * same array is how a write that changed nothing says so, and such a write
 * wakes nobody: `useSyncExternalStore` decides whether to render by comparing
 * the reference it last read against this one, so a fresh array for an
 * unchanged queue re-renders every region on the page, and a queue mutated in
 * place re-renders none of them.
 *
 * The listeners are iterated over a copy, because one may unsubscribe while
 * they run — React unmounts a `useSyncExternalStore` subscriber by calling
 * exactly that unsubscribe — and a `Set` mutated mid-iteration skips entries.
 * The membership test is against the live set, so one that has just left is
 * not called anyway.
 */
function writeQueue(
  change: (current: $ReadOnlyArray<Notification>) => $ReadOnlyArray<Notification>,
): void {
  const next = change(queue);
  if (next === queue) {
    return;
  }
  queue = next;
  for (const watcher of Array.from(watchers)) {
    if (watchers.has(watcher)) {
      watcher();
    }
  }
}

/** Ids are this module's, because `useId` needs a component and `toast()` is not one. */
let sequence = 0;

/**
 * Show a notification, and hand back the id that identifies it later.
 *
 * Callable from anywhere — an event handler, a `catch`, a Server Action's
 * error path — because it is a function in a module rather than a hook.
 *
 *     toast("Saved");
 *     toast("Could not save", { urgency: "assertive" });
 *     const id = toast("Uploading…", { duration: null });
 *     updateToast(id, { content: "Uploaded", duration: 4000 });
 */
export function toast(content: React.Node, options?: ToastOptions): string {
  sequence += 1;
  const notification: Notification = {
    id: `uf-toast-${String(sequence)}`,
    content,
    urgency: options?.urgency ?? "polite",
    duration: options?.duration === undefined ? DEFAULT_DURATION : options.duration,
  };
  writeQueue((current) => [...current, notification]);
  return notification.id;
}

/**
 * Change a notification that is already queued.
 *
 * "Uploading…" becoming "Uploaded" is one notification that changed, not two
 * notifications — and a reader who is told the second thing without the first
 * disappearing has been told the upload is both in progress and finished.
 *
 * Changing the content of a notification that is on screen re-announces the
 * region it is in, which is the point.
 */
export function updateToast(id: string, changes: ToastChanges): void {
  writeQueue((current) =>
    current.map((each) =>
      each.id === id
        ? {
            id: each.id,
            content: changes.content === undefined ? each.content : changes.content,
            urgency: changes.urgency ?? each.urgency,
            duration: changes.duration === undefined ? each.duration : changes.duration,
          }
        : each,
    ),
  );
}

/** Take a notification away, whether it expired, was dismissed, or was acted on. */
export function dismissToast(id: string): void {
  writeQueue((current) => {
    const left = current.filter((each) => each.id !== id);
    // The same array back when nothing matched, so a stray dismissal is not a
    // new snapshot and does not render every region on the page.
    return left.length === current.length ? current : left;
  });
}

/**
 * Empty the queue.
 *
 * For a route change that makes every pending notification stale, and for a
 * test: the queue is one module-level value, so what one test queued is still
 * there in the next one unless something clears it.
 */
export function dismissAllToasts(): void {
  writeQueue(() => NONE);
}

/** Module-level and therefore stable, which is what stops React re-subscribing. */
function subscribeToQueue(listener: () => void): () => void {
  watchers.add(listener);
  return () => {
    watchers.delete(listener);
  };
}

/**
 * The current queue.
 *
 * Used for the server snapshot as well as the client one, because the value is
 * the same frozen array on both sides until something writes: hydration then
 * compares like with like rather than against a placeholder. On a server it is
 * always `NONE`, since nothing on a server calls `toast()` — this module is
 * `"use client"`.
 */
function readQueue(): $ReadOnlyArray<Notification> {
  return queue;
}

const NotificationContext: React.Context<Notification | null> = createContext(null);

hook useNotification(part: string): Notification {
  const notification = useContext(NotificationContext);
  if (notification == null) {
    throw new Error(`${part} must be rendered inside a Toast.Region's children`);
  }
  return notification;
}

/** The ids of a notification's own parts, so it only names ones that exist. */
type ToastParts = {|
  readonly titleId: string,
  readonly descriptionId: string,
  readonly registerTitle: (present: boolean) => void,
  readonly registerDescription: (present: boolean) => void,
|};

const ToastPartsContext: React.Context<ToastParts | null> = createContext(null);

/**
 * The notifications, and the two regions that were watching before them.
 *
 * Render it once, in the layout. It is a page-level surface: every `toast()`
 * anywhere in the application arrives here, and two of these would show the
 * same notifications twice and bind `F6` twice.
 *
 * `children` is a function rather than elements, because the notifications are
 * not known when the region is written — and it is typed `renders ToastRoot`,
 * so a `<div>` where a notification belongs is a type error rather than a
 * stack of notifications a reader cannot dismiss.
 */
export component ToastRegion(
  children: (notification: Notification) => renders ToastRoot,
  label?: string = "Notifications",
  limit?: number = 3,
  ...rest: Rest
) {
  const queued = useSyncExternalStore(subscribeToQueue, readQueue, readQueue);
  const region = useElementRef<HTMLElement>();
  // Where `F6` came from, so pressing it again gives focus back.
  const cameFrom = useRef<HTMLElement | null>(null);
  const passed = withoutComposed(rest, ["ref"]);

  useKeyCombo(
    "f6",
    () => {
      const element = region.current;
      if (element == null) {
        return;
      }
      const active: $FlowFixMe = element.ownerDocument.activeElement;
      if (active != null && element.contains(active)) {
        const back = cameFrom.current;
        cameFrom.current = null;
        back?.focus?.();
        return;
      }
      cameFrom.current = active;
      element.focus();
    },
    // The one case that matters is a notification arriving while the reader is
    // filling in a form, so this must fire from inside a text field.
    { whileTyping: true },
  );

  // The oldest first, so a burst of notifications is a queue rather than a
  // pile that hides what arrived first. What is over the limit is not rendered
  // at all, which is also what keeps its countdown from running before anyone
  // has seen it.
  const shown = queued.slice(0, limit);

  const place = (notification: Notification) => (
    <NotificationContext.Provider key={notification.id} value={notification}>
      {children(notification)}
    </NotificationContext.Provider>
  );

  return (
    <div
      {...passed}
      aria-label={label}
      ref={composeRefs(rest.ref, (element) => {
        region.current = element;
      })}
      role="region"
      // So `F6` can put focus on the region itself, including while it is
      // empty, without the region joining the tab order.
      tabIndex={-1}
    >
      <div aria-atomic="true" aria-live="polite" role="status">
        {shown.filter((each) => each.urgency === "polite").map(place)}
      </div>
      <div aria-atomic="true" aria-live="assertive" role="alert">
        {shown.filter((each) => each.urgency === "assertive").map(place)}
      </div>
    </div>
  );
}

/**
 * One notification, and the countdown that stops.
 *
 * `role="group"` named by its `Toast.Title`, which is what turns a stack of
 * three into three things a reader can move between after `F6` rather than one
 * run of text. Named and described only while those parts are rendered, for
 * the reason every part of this package repeats: an `aria-labelledby` naming
 * an id that is not in the document makes a screen reader announce nothing at
 * all.
 */
export component ToastRoot(children: React.Node, ...rest: Rest) {
  const notification = useNotification("Toast.Root");
  const base = useId();
  const element = useElementRef<HTMLElement>();
  const [titled, setTitled] = useState(false);
  const [described, setDescribed] = useState(false);
  const passed = withoutComposed(rest, ["ref"]);

  const hovered = useHover(element);
  const focusInside = useFocusWithin(element);
  const documentVisible = useDocumentVisible();
  const paused = hovered || focusInside || !documentVisible;

  const { duration, id } = notification;
  // What is left of the countdown. State rather than a ref because the render
  // hands it to `useTimeout`, and reading a ref during a render is a rule this
  // package does not break.
  const [left, setLeft] = useState<number | null>(duration);
  const startedAt = useRef(0);

  useTimeout(() => dismissToast(id), paused || left == null ? null : left);

  useEffect(() => {
    if (paused) {
      // Bank what has run. Without this the unpause re-arms `useTimeout` with
      // the whole duration again, and a pointer resting near the stack keeps a
      // notification on screen for ever.
      setLeft((current) =>
        current == null ? null : Math.max(0, current - (Date.now() - startedAt.current)),
      );
      return;
    }
    startedAt.current = Date.now();
  }, [paused]);

  // A notification whose duration was changed under it — "Uploading…" with no
  // duration becoming "Uploaded" with one — starts its countdown from there.
  useEffect(() => {
    setLeft(duration);
    startedAt.current = Date.now();
  }, [duration]);

  const parts = useMemo(
    () => ({
      titleId: `${base}-title`,
      descriptionId: `${base}-description`,
      registerTitle: setTitled,
      registerDescription: setDescribed,
    }),
    [base],
  );

  return (
    <ToastPartsContext.Provider value={parts}>
      <div
        {...passed}
        aria-describedby={described ? parts.descriptionId : undefined}
        aria-labelledby={titled ? parts.titleId : undefined}
        ref={composeRefs(rest.ref, (node) => {
          element.current = node;
        })}
        role="group"
      >
        {children}
      </div>
    </ToastPartsContext.Provider>
  );
}

/** What the notification is about, and the name of its group. */
export component ToastTitle(children: React.Node, ...rest: Rest) {
  const parts = useToastParts("Toast.Title");
  useRegistration(parts.registerTitle);

  return (
    <div {...rest} id={parts.titleId}>
      {children}
    </div>
  );
}

/** The rest of it, and the group's description. */
export component ToastDescription(children: React.Node, ...rest: Rest) {
  const parts = useToastParts("Toast.Description");
  useRegistration(parts.registerDescription);

  return (
    <div {...rest} id={parts.descriptionId}>
      {children}
    </div>
  );
}

/**
 * The thing the notification offers to do: Undo, Retry, View.
 *
 * Taking the action dismisses the notification, because a notification whose
 * offer has been accepted is describing something that is no longer true.
 *
 * A notification carrying one of these should be given `duration: null`. The
 * countdown stopping while focus is inside is what makes the button reachable
 * at all; it is not a promise that four seconds was enough time to decide.
 */
export component ToastAction(children: React.Node, ...rest: Rest) {
  const notification = useNotification("Toast.Action");
  const passed = withoutComposed(rest, ["onClick"]);

  return (
    <button
      {...passed}
      onClick={composeHandlers(rest.onClick, () => dismissToast(notification.id))}
      type="button"
    >
      {children}
    </button>
  );
}

/**
 * The button that takes the notification away.
 *
 * `label` becomes the accessible name, and it has a default because the close
 * on a notification is an icon almost every time — and an icon-only button
 * with no name is announced as "button", which is a control a reader can find
 * and cannot identify. `Dialog.Close` has no default for the opposite reason:
 * its content is words.
 *
 * If you render visible text inside this, pass the same words as `label`.
 * WCAG 2.5.3 asks that the name contain what a reader sees, and an
 * `aria-label` that says "Dismiss" over a button that says "Close" breaks the
 * speech reader who says "click Close" out loud.
 */
export component ToastClose(children?: React.Node, label?: string = "Dismiss", ...rest: Rest) {
  const notification = useNotification("Toast.Close");
  const passed = withoutComposed(rest, ["onClick"]);

  return (
    <button
      {...passed}
      aria-label={label}
      onClick={composeHandlers(rest.onClick, () => dismissToast(notification.id))}
      type="button"
    >
      {children}
    </button>
  );
}

hook useToastParts(part: string): ToastParts {
  const parts = useContext(ToastPartsContext);
  if (parts == null) {
    throw new Error(`${part} must be rendered inside a Toast.Root`);
  }
  return parts;
}

/** Tell a `Toast.Root` that one of the parts it names is in the document. */
hook useRegistration(register: (present: boolean) => void): void {
  useEffect(() => {
    register(true);
    return () => register(false);
  }, [register]);
}
