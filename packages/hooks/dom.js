// @flow
//
// `@uniflowed/hooks/dom`: watching one node.
//
// Each of these takes a ref rather than returning one, so a component can put
// several on the same element and can hand the ref to something else as well.
// The listener is attached in a layout effect, so it is in place before the
// browser paints — a click that lands in the same frame as the mount is a real
// case on a touch screen.
//
// # What belongs in this module
//
// A hook whose subject is a particular element the caller is holding: listen
// to it, measure it, notice a pointer over it, notice it entering the
// viewport, notice its children change, notice a press that lasts. The
// signature is the giveaway — if it takes a ref, it is here.
//
// The neighbour it is most often confused with is `browser.js`, which reads
// the ambient environment: the window's size, its scroll offset, whether the
// document is visible, what the reader's media queries say. Those need no ref
// because there is only one of the thing they read, and they are built on
// `useSyncExternalStore` with a stated server value. These need a ref because
// there are as many answers as there are elements, and they are built on
// effects because there is nothing to read until one is mounted.
//
// The file was called `element.js`, which named the argument rather than the
// job and left `browser.js` looking like its opposite when it is its sibling.
//
// # Before hydration
//
// Nothing here runs. Every hook in this file does its work in an effect, and
// effects do not run during a prerender — so each one reports its stated
// starting value (`false` for a pointer or a focus, zero for a size or a
// scroll offset) in the server's HTML and in the client's first render, which
// is what makes them agree. There is no `typeof window` in this file: a ref is
// null on a server for the same reason it is null before mount, and one
// branch covers both.

import { useEffect, useMemo, useRef, useState } from "@uniflowed/react";

import type { ScrollOffset, Size } from "./browser.js";
import { browserWindow } from "./browser.js";
import { useIsomorphicLayoutEffect, useStableCallback } from "./lifecycle.js";

/** A ref object these hooks read: what `useRef` and `useElementRef` return. */
export type Ref<T> = { current: T | null };

/**
 * What a listener can be attached to.
 *
 * A ref, or a function that finds the target. The function form is what covers
 * the window and the document, which no ref points at.
 */
export type ListenerTarget<T> = Ref<T> | (() => T | null) | null;

/**
 * The part of `addEventListener`'s options a hook here passes on.
 *
 * `signal` is deliberately absent: these hooks remove their own listener in the
 * effect's cleanup, and a second, independent way to remove it would be a
 * second thing that can be wrong.
 */
export type ListenerOptions = {|
  readonly capture?: boolean,
  readonly passive?: boolean,
  readonly once?: boolean,
|};

/**
 * Listen to an event on a target, cleaning up after itself.
 *
 * The handler is stabilised, so passing an inline arrow does not tear the
 * listener down and set it up again on every render — which is the bug this
 * hook exists to prevent and the reason it does not take a dependency array.
 *
 * The target is read once, when the listener is attached. Passing a ref and
 * later pointing it at a different element does not move the listener; a
 * component whose target changes should let the element unmount and mount
 * again, which is what React does anyway when the element is conditional.
 */
export hook useEventListener<TTarget extends EventTarget>(
  target: ListenerTarget<TTarget>,
  name: string,
  handler: (event: Event) => mixed,
  options?: ListenerOptions,
): void {
  const stable = useStableCallback(handler);
  const find = useStableCallback(() =>
    typeof target === "function" ? target() : (target?.current ?? null),
  );
  // A ref is a stable object and a function target is not, so only the ref
  // form goes into the dependency array: an inline `() => window` would
  // otherwise re-subscribe on every render, and that is the whole bug.
  const ref = typeof target === "function" ? null : target;
  const capture = options?.capture ?? false;
  const passive = options?.passive;
  const once = options?.once ?? false;

  useIsomorphicLayoutEffect(() => {
    const node = find();
    if (node == null) {
      return;
    }
    const listener = (event: Event) => {
      stable(event);
    };
    node.addEventListener(name, listener, { capture, passive, once });
    return () => node.removeEventListener(name, listener, { capture });
  }, [find, ref, name, stable, capture, passive, once]);
}

/**
 * Call `handler` when a press lands outside `ref`.
 *
 * `pointerdown` rather than `click`, because a menu that closes on click stays
 * open for the whole press — and because a click whose press started inside
 * the menu and ended outside it should not close it.
 */
export hook useClickOutside(ref: Ref<HTMLElement>, handler: (event: Event) => mixed): void {
  const stable = useStableCallback(handler);

  useEffect(() => {
    const document = browserWindow()?.document;
    if (document == null) {
      return;
    }
    const listener = (event: Event) => {
      const node = ref.current;
      const target = event.target;
      if (node != null && target instanceof Node && !node.contains(target)) {
        stable(event);
      }
    };
    document.addEventListener("pointerdown", listener);
    return () => document.removeEventListener("pointerdown", listener);
  }, [ref, stable]);
}

/** Whether the pointer is over the element. */
export hook useHover(ref: Ref<HTMLElement>): boolean {
  const [hovered, setHovered] = useState(false);
  useEventListener(ref, "pointerenter", () => setHovered(true));
  useEventListener(ref, "pointerleave", () => setHovered(false));
  return hovered;
}

/** Whether focus is inside the element. */
export hook useFocusWithin(ref: Ref<HTMLElement>): boolean {
  const [within, setWithin] = useState(false);
  useEventListener(ref, "focusin", () => setWithin(true));
  useEventListener(ref, "focusout", () => setWithin(false));
  return within;
}

/**
 * Where a pointer event happened, or `null` for one that carries no position.
 *
 * `MouseEvent` rather than `PointerEvent`, because a pointer event is a mouse
 * event by specification and the narrower name is not defined in every
 * environment a uf test runs in — an `instanceof` against a name that does not
 * exist is a `ReferenceError`, not a `false`.
 */
function pointOf(event: Event): {| x: number, y: number |} | null {
  return event instanceof MouseEvent ? { x: event.clientX, y: event.clientY } : null;
}

/**
 * Call `handler` when a press on the element lasts.
 *
 * Cancelled by letting go, by the pointer leaving, and by the pointer moving
 * further than `moveThreshold` — a press that turns into a scroll or a drag is
 * not a long press, and a version that only watched for `pointerup` fires a
 * context menu in the middle of a fling.
 *
 * The handler is called once per press, while the finger is still down, which
 * is when a long press is supposed to be felt.
 */
export hook useLongPress(
  ref: Ref<HTMLElement>,
  handler: (event: Event) => mixed,
  options?: {| readonly delay?: number, readonly moveThreshold?: number |},
): void {
  const stable = useStableCallback(handler);
  const delay = options?.delay ?? 500;
  const moveThreshold = options?.moveThreshold ?? 10;

  // Written and read only from event handlers and the effect's cleanup, never
  // during a render, so a render React throws away cannot see a press.
  const pending = useRef<TimeoutID | null>(null);
  const origin = useRef<{| x: number, y: number |} | null>(null);

  const cancel = useStableCallback(() => {
    if (pending.current != null) {
      clearTimeout(pending.current);
      pending.current = null;
    }
    origin.current = null;
  });

  useEventListener(ref, "pointerdown", (event: Event) => {
    cancel();
    origin.current = pointOf(event);
    pending.current = setTimeout(() => {
      pending.current = null;
      stable(event);
    }, delay);
  });

  useEventListener(ref, "pointermove", (event: Event) => {
    const start = origin.current;
    const moved = pointOf(event);
    if (start == null || moved == null) {
      return;
    }
    if (Math.hypot(moved.x - start.x, moved.y - start.y) > moveThreshold) {
      cancel();
    }
  });

  useEventListener(ref, "pointerup", () => cancel());
  useEventListener(ref, "pointercancel", () => cancel());
  useEventListener(ref, "pointerleave", () => cancel());

  useEffect(() => cancel, [cancel]);
}

/**
 * The element's size, as the browser measures it.
 *
 * A `ResizeObserver` rather than a window resize listener, because an element
 * changes size when its content changes, when a sibling grows, and when a
 * container query fires — none of which resizes the window.
 */
export hook useElementSize(ref: Ref<HTMLElement>): Size {
  const [size, setSize] = useState({ width: 0, height: 0 });

  useIsomorphicLayoutEffect(() => {
    const node = ref.current;
    const Observer = browserWindow()?.ResizeObserver;
    if (node == null || Observer == null) {
      return;
    }
    const observer = new Observer((entries: $ReadOnlyArray<ResizeObserverEntry>) => {
      const entry = entries[0];
      if (entry == null) {
        return;
      }
      const box = entry.contentRect;
      // Only on a real change: an observer that fires with the same numbers
      // would re-render forever.
      setSize((current) =>
        current.width === box.width && current.height === box.height
          ? current
          : { width: box.width, height: box.height },
      );
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [ref]);

  return size;
}

/** Whether the element is in the viewport. */
export hook useIntersecting(
  ref: Ref<HTMLElement>,
  options?: {| readonly rootMargin?: string, readonly threshold?: number |},
): boolean {
  const [intersecting, setIntersecting] = useState(false);
  const rootMargin = options?.rootMargin;
  const threshold = options?.threshold;

  useEffect(() => {
    const node = ref.current;
    const Observer = browserWindow()?.IntersectionObserver;
    if (node == null || Observer == null) {
      return;
    }
    const observer = new Observer(
      (entries: Array<IntersectionObserverEntry>) => {
        const entry = entries[0];
        if (entry != null) {
          setIntersecting(entry.isIntersecting);
        }
      },
      { rootMargin, threshold },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [ref, rootMargin, threshold]);

  return intersecting;
}

/** What part of the tree under the element to watch. */
export type MutationOptions = {|
  /** Children added or removed. The default, unless another kind is asked for. */
  readonly childList?: boolean,
  /** Descendants as well as the element itself. */
  readonly subtree?: boolean,
  readonly attributes?: boolean,
  readonly characterData?: boolean,
  /** Only these attributes, where `attributes` is on. */
  readonly attributeFilter?: $ReadOnlyArray<string>,
|};

/**
 * Call `handler` when the element's markup changes.
 *
 * The last resort of the three observers, and worth saying so: a size is a
 * `ResizeObserver`, a position is an `IntersectionObserver`, and this is for
 * the case where something outside React edits the DOM — a third-party widget,
 * a browser extension, a `contenteditable`. Watching a tree React owns in
 * order to learn about React's own updates is a mistake this hook cannot
 * prevent but should not encourage.
 *
 * `attributeFilter` is compared by its contents rather than its identity, so
 * an array written inline in the call does not re-observe on every render.
 */
export hook useMutationObserver(
  ref: Ref<HTMLElement>,
  handler: (records: $ReadOnlyArray<MutationRecord>) => mixed,
  options?: MutationOptions,
): void {
  const stable = useStableCallback(handler);
  const attributes = options?.attributes ?? false;
  const characterData = options?.characterData ?? false;
  const childList = options?.childList ?? !(attributes || characterData);
  const subtree = options?.subtree ?? false;
  const attributeFilter = options?.attributeFilter;
  const filterKey = attributeFilter == null ? null : attributeFilter.join(",");

  useEffect(() => {
    const node = ref.current;
    const Observer = browserWindow()?.MutationObserver;
    if (node == null || Observer == null) {
      return;
    }
    // Flow's `MutationObserverInit` requires one of the three kinds to be
    // literally `true`, which is the specification's own rule: an observer
    // that watches nothing throws. The branches are that rule, not a style.
    const filter = filterKey == null ? undefined : filterKey.split(",");
    const init: MutationObserverInit = childList
      ? { childList: true, subtree, attributes, characterData, attributeFilter: filter }
      : attributes
        ? { attributes: true, subtree, characterData, attributeFilter: filter }
        : { characterData: true, subtree };
    const observer = new Observer((records: Array<MutationRecord>) => {
      stable(records);
    });
    observer.observe(node, init);
    return () => observer.disconnect();
  }, [ref, stable, childList, subtree, attributes, characterData, filterKey]);
}

/**
 * How far the element has been scrolled.
 *
 * The element's own offset, not the page's — `useWindowScroll` is the page's,
 * and lives in `browser.js` because there is only one page.
 *
 * A passive listener, because a scroll handler that could call
 * `preventDefault` blocks scrolling on a touch screen until it has run; and a
 * layout effect for the first reading, because a container restored to a saved
 * offset should not report zero for one frame.
 */
export hook useScroll(ref: Ref<HTMLElement>): ScrollOffset {
  const [offset, setOffset] = useState({ x: 0, y: 0 });

  const read = useStableCallback(() => {
    const node = ref.current;
    if (node == null) {
      return;
    }
    const x = node.scrollLeft;
    const y = node.scrollTop;
    setOffset((current) => (current.x === x && current.y === y ? current : { x, y }));
  });

  useIsomorphicLayoutEffect(read, [read]);
  useEventListener(ref, "scroll", () => read(), { passive: true });

  return offset;
}

/** A ref for one of the hooks above, typed for the element you will attach it to. */
export hook useElementRef<T extends HTMLElement>(): Ref<T> {
  return useRef<T | null>(null);
}

/**
 * The element a ref points at, as a value a render can depend on.
 *
 * A ref is not state: React does not re-render when `current` changes, and
 * reading `ref.current` during a render is a rule violation because the render
 * that reads it may be one React throws away. A component that has to *render*
 * something derived from its own element — a measurement, a portal target —
 * needs the element as state, which is what a callback ref gives.
 *
 * The returned function is stable, so passing it as `ref={setNode}` does not
 * detach and reattach on every render.
 */
export hook useElementState<T extends HTMLElement>(): [T | null, (node: T | null) => void] {
  const [node, setNode] = useState<T | null>(null);
  const attach = useStableCallback((next: T | null) => setNode(next));
  return useMemo(() => [node, attach], [node, attach]);
}
