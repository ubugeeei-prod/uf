// @flow
//
// Watching a node.
//
// Each of these takes a ref rather than returning one, so a component can put
// several on the same element and can hand the ref to something else as well.
// The listener is attached in a layout effect, so it is in place before the
// browser paints — a click that lands in the same frame as the mount is a real
// case on a touch screen.

import { useEffect, useRef, useState } from "@uniflowed/react";

import { useIsomorphicLayoutEffect, useStableCallback } from "./lifecycle.js";

type Ref<T> = { current: T | null };

/**
 * The window these hooks read constructors from.
 *
 * In a browser `globalThis` is the window; anywhere a document was installed
 * onto another host's global it is not, and `globalThis.ResizeObserver` is
 * undefined while `window.ResizeObserver` is there.
 */
function windowOf(): any {
  return globalThis.window ?? globalThis;
}

/**
 * Listen to an event on a target, cleaning up after itself.
 *
 * `target` may be a ref, an element, or a function returning either, so this
 * covers the window, the document, and a node that does not exist yet at the
 * time the hook is called.
 */
export function useEventListener<TEvent extends Event>(
  target: Ref<EventTarget> | EventTarget | (() => EventTarget | null) | null,
  name: string,
  handler: (event: TEvent) => mixed,
  options?: AddEventListenerOptions,
): void {
  const stable = useStableCallback(handler);
  const capture = options?.capture ?? false;
  const passive = options?.passive;
  const once = options?.once ?? false;

  useIsomorphicLayoutEffect(() => {
    const node = resolve(target);
    if (node == null) {
      return;
    }
    const listener = (event: Event) => stable(event as any);
    node.addEventListener(name, listener, { capture, passive, once });
    return () => node.removeEventListener(name, listener, { capture });
  }, [target, name, stable, capture, passive, once]);
}

/**
 * Call `handler` when a press lands outside `ref`.
 *
 * `pointerdown` rather than `click`, because a menu that closes on click stays
 * open for the whole press — and because a click whose press started inside
 * the menu and ended outside it should not close it.
 */
export function useClickOutside(ref: Ref<HTMLElement>, handler: (event: Event) => mixed): void {
  const stable = useStableCallback(handler);

  useEffect(() => {
    if (typeof globalThis.document === "undefined") {
      return;
    }
    const listener = (event: Event) => {
      const node = ref.current;
      const target: any = event.target;
      if (node != null && target != null && !node.contains(target)) {
        stable(event);
      }
    };
    globalThis.document.addEventListener("pointerdown", listener);
    return () => globalThis.document.removeEventListener("pointerdown", listener);
  }, [ref, stable]);
}

/** Whether the pointer is over the element. */
export function useHover(ref: Ref<HTMLElement>): boolean {
  const [hovered, setHovered] = useState(false);
  useEventListener(ref, "pointerenter", () => setHovered(true));
  useEventListener(ref, "pointerleave", () => setHovered(false));
  return hovered;
}

/** Whether focus is inside the element. */
export function useFocusWithin(ref: Ref<HTMLElement>): boolean {
  const [within, setWithin] = useState(false);
  useEventListener(ref, "focusin", () => setWithin(true));
  useEventListener(ref, "focusout", () => setWithin(false));
  return within;
}

/**
 * The element's size, as the browser measures it.
 *
 * A `ResizeObserver` rather than a window resize listener, because an element
 * changes size when its content changes, when a sibling grows, and when a
 * container query fires — none of which resizes the window.
 */
export function useElementSize(ref: Ref<HTMLElement>): {|
  readonly width: number,
  readonly height: number,
|} {
  const [size, setSize] = useState({ width: 0, height: 0 });

  useIsomorphicLayoutEffect(() => {
    const node = ref.current;
    if (node == null || typeof windowOf().ResizeObserver !== "function") {
      return;
    }
    const observer = new (windowOf().ResizeObserver)((entries) => {
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
export function useIntersecting(
  ref: Ref<HTMLElement>,
  options?: {| readonly rootMargin?: string, readonly threshold?: number |},
): boolean {
  const [intersecting, setIntersecting] = useState(false);
  const rootMargin = options?.rootMargin;
  const threshold = options?.threshold;

  useEffect(() => {
    const node = ref.current;
    if (node == null || typeof windowOf().IntersectionObserver !== "function") {
      return;
    }
    const observer = new (windowOf().IntersectionObserver)(
      (entries) => {
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

function resolve(
  target: Ref<EventTarget> | EventTarget | (() => EventTarget | null) | null,
): EventTarget | null {
  if (target == null) {
    return null;
  }
  if (typeof target === "function") {
    return target();
  }
  if (typeof (target as any).addEventListener === "function") {
    return target as any;
  }
  return (target as any).current ?? null;
}

/** A ref for one of the hooks above, typed for the element you will attach it to. */
export function useElementRef<T extends HTMLElement>(): Ref<T> {
  return useRef<T | null>(null);
}
