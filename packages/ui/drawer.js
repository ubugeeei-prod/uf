// @flow
//
// A drawer: the sheet you can drag away, and the one with a specification
// attached.
//
// It is `sheet.js` — same edge, same modal promises, same `data-side` — plus a
// gesture. The gesture is the whole of what is new, and a gesture is the part
// of a component most likely to be inaccessible while looking polished:
//
//   * **WCAG 2.2 SC 2.5.7, *Dragging Movements*.** Anything achievable by
//     dragging must also be achievable with a single pointer and no drag. So
//     drag-to-dismiss is an *addition to* a close button and never a
//     replacement for one, and `Drawer.Body` raises when a `Drawer.Handle` is
//     rendered without a `Drawer.Close` beside it. A drawer that can only be
//     dismissed by dragging is inaccessible, and it is inaccessible in the way
//     that gets shipped: it demonstrates beautifully.
//   * **WCAG 2.1.1, *Keyboard*.** Every snap point the drag can reach, the
//     arrow keys reach. `Drawer.Handle` is a `role="slider"` over the snap
//     points, with `Home` and `End` at the ends — which is also why it has a
//     `label`: a slider with no accessible name is announced as "slider".
//     Pressing the closing key at the smallest snap point closes the drawer,
//     because "drag it off the edge" has to be a key as well.
//   * **`prefers-reduced-motion`.** A drawer that slides and springs is motion
//     the reader may have asked their system not to make. `usePrefersReducedMotion`
//     from `@uniflowed/hooks/browser` puts `data-reduced-motion="true"` on the
//     body, and the stylesheet drops the transition. The drag itself still
//     follows the finger: direct manipulation is not animation, and freezing it
//     would make the drawer feel broken rather than calm.
//
// # Snap points are indices, and the type says so
//
// `snapPoints` is a list of fractions of the drawer's full size, ascending —
// `[0.4, 1]` is "peek, then full". The *state* is the index into that list
// rather than the fraction, because the arrow keys move by one snap point and
// a slider whose value is `0.4` has to be told what the next value is. The
// index is also what `aria-valuenow` can be: `aria-valuemin={0}` and
// `aria-valuemax={snapPoints.length - 1}` are true about a list, and
// `aria-valuetext` is what says "40%" to a reader.
//
// # Where the numbers go
//
// `--uf-drawer-snap` (the current fraction) and `--uf-drawer-drag` (how far the
// finger has moved, in pixels) are written straight onto the element rather
// than put in state, for the reason `internal/anchor.js` gives about a
// placement: the second of them changes on every pointer frame, and
// re-rendering the drawer and everything in it to move a box is the cost this
// package does not pay. React owns neither property.

"use client";

import * as React from "@uniflowed/react";
import { createContext, useContext, useEffect, useMemo, useRef, useState } from "@uniflowed/react";
import { usePrefersReducedMotion } from "@uniflowed/hooks/browser";

import type { Edge } from "./sheet.js";
import type { RenderProp, Rest } from "./internal/merge-props.js";
import {
  composeHandlers,
  composeRefs,
  forwarded,
  withProps,
  withoutComposed,
} from "./internal/merge-props.js";
import {
  SheetBody,
  SheetClose,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetOverlay,
  SheetRoot,
  SheetTitle,
  SheetTrigger,
} from "./sheet.js";
import { useControlled } from "./internal/controlled-state.js";

export type { Edge } from "./sheet.js";

/**
 * The whole drawer, and the only snap point a caller who asked for none gets.
 *
 * Frozen at module scope rather than defaulted inline, so the default is one
 * array rather than a fresh one per render — which would make every memo keyed
 * on `snapPoints` miss.
 */
const FULLY_OPEN: $ReadOnlyArray<number> = Object.freeze([1]);

/** How far along its own size a drag has to travel to change the snap point. */
const DRAG_THRESHOLD = 0.25;

type DrawerState = {|
  readonly side: Edge,
  readonly snapPoints: $ReadOnlyArray<number>,
  readonly snapIndex: number,
  readonly setSnapIndex: (next: number) => void,
  readonly close: () => void,
  readonly bodyRef: { current: HTMLElement | null },
  /**
   * How many `Drawer.Close`es and `Drawer.Handle`s are in the document.
   *
   * Counted refs rather than state, for the reason `alert-dialog.js` gives: a
   * child's effect runs before its parent's, so `Drawer.Body` can ask about
   * both on the commit that mounted them, and nothing renders either number.
   */
  readonly closes: { current: number },
  readonly handles: { current: number },
|};

const DrawerContext: React.Context<DrawerState | null> = createContext(null);

/**
 * The drawer a part belongs to.
 *
 * Raising rather than returning null, for the reason `useDialog` gives: a
 * `Drawer.Handle` outside a root would render a slider over no snap points.
 */
hook useDrawer(part: string): DrawerState {
  const state = useContext(DrawerContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Drawer.Root`);
  }
  return state;
}

/**
 * The drawer, open or closed, at one of its snap points.
 *
 * It owns `open` rather than letting `Dialog.Root` own it — and hands it down
 * as a controlled prop — because the drag has to be able to close the drawer
 * from a pointer handler, and the dialog's own state is not reachable from
 * outside its parts. Both arrangements still work for the caller: `open` and
 * `onOpenChange` behave exactly as they do everywhere else in this package,
 * because `internal/controlled-state.js` is what answers here too.
 */
export component DrawerRoot(
  children: React.Node,
  defaultOpen?: boolean = false,
  defaultSnapPoint?: number = 0,
  onOpenChange?: (open: boolean) => void,
  onSnapPointChange?: (index: number) => void,
  open?: boolean,
  side?: Edge = "bottom",
  snapPoint?: number,
  snapPoints?: $ReadOnlyArray<number> = FULLY_OPEN,
) {
  const [isOpen, setOpen] = useControlled(open, defaultOpen, onOpenChange);
  const [snapIndex, setSnapIndex] = useControlled(snapPoint, defaultSnapPoint, onSnapPointChange);
  const bodyRef = useRef<HTMLElement | null>(null);
  const closes = useRef(0);
  const handles = useRef(0);

  const state = useMemo(
    () => ({
      bodyRef,
      close: () => setOpen(false),
      closes,
      handles,
      setSnapIndex,
      side,
      snapIndex,
      snapPoints,
    }),
    [setOpen, setSnapIndex, side, snapIndex, snapPoints],
  );

  return (
    <DrawerContext.Provider value={state}>
      <SheetRoot onOpenChange={setOpen} open={isOpen} side={side}>
        {children}
      </SheetRoot>
    </DrawerContext.Provider>
  );
}

/** What opens it, and what focus comes back to when it closes. */
export component DrawerTrigger(children: React.Node, render?: RenderProp, ...rest: Rest) {
  return (
    <SheetTrigger {...forwarded(rest)} render={render}>
      {children}
    </SheetTrigger>
  );
}

/** The backdrop. It carries the edge, the same as a sheet's. */
export component DrawerOverlay(render?: RenderProp, ...rest: Rest) {
  return <SheetOverlay {...forwarded(rest)} render={render} />;
}

/**
 * The drawer itself: a sheet, at a snap point, that can be dragged.
 *
 * The raise is the WCAG 2.5.7 clause, enforced rather than documented. It fires
 * only when a `Drawer.Handle` is rendered, because a drawer with no handle has
 * no drag to provide an alternative to — and a drawer with a handle and no
 * `Drawer.Close` has a gesture that is the only way out.
 */
export component DrawerBody(children: React.Node, render?: RenderProp, ...rest: Rest) {
  const drawer = useDrawer("Drawer.Body");
  const { bodyRef, snapIndex, snapPoints } = drawer;
  const reducedMotion = usePrefersReducedMotion();
  const fraction = snapPoints[snapIndex] ?? 1;

  // Written rather than rendered, for the reason the module header gives: this
  // is the pair `--uf-drawer-drag` moves between, and putting either in a
  // `style` prop would hand React a property the pointer handler also writes.
  useEffect(() => {
    const body = bodyRef.current;
    if (body == null) {
      return;
    }
    body.style.setProperty("--uf-drawer-snap", String(fraction));
    body.style.setProperty("--uf-drawer-drag", "0px");
  }, [bodyRef, fraction]);

  return (
    <SheetBody
      {...forwarded(rest)}
      data-reduced-motion={reducedMotion ? "true" : undefined}
      data-snap-point={String(snapIndex)}
      ref={composeRefs(rest.ref, (element: HTMLElement | null) => {
        bodyRef.current = element;
      })}
      render={render}
    >
      {children}
      <RequireCloseForTheDrag />
    </SheetBody>
  );
}

/**
 * WCAG 2.5.7, asked where it can be answered.
 *
 * Inside `Sheet.Body` and last, for the reason `alert-dialog.js`'s
 * `RequireDescription` gives: a drawer that has not been opened has neither a
 * handle nor a close button in the document, so the question is only meaningful
 * once the body is showing, and every part above this has counted itself by the
 * time this asks.
 *
 * A drawer with no handle has no drag, and a rule about dragging has nothing to
 * say about it — which is why the raise is conditional on there being one
 * rather than on there being a close button.
 */
component RequireCloseForTheDrag() {
  const drawer = useDrawer("Drawer.Body");
  const { closes, handles } = drawer;

  useEffect(() => {
    if (handles.current > 0 && closes.current === 0) {
      throw new Error(
        "Drawer.Body has a Drawer.Handle and no Drawer.Close: WCAG 2.2 SC 2.5.7 " +
          "requires anything achievable by dragging to be achievable without a " +
          "drag, so drag-to-dismiss is an addition to a close button and never a " +
          "replacement for one.",
      );
    }
  }, [closes, handles]);

  return null;
}

/** The top of the drawer, where the handle usually goes. */
export component DrawerHeader(children: React.Node, render?: RenderProp, ...rest: Rest) {
  return (
    <SheetHeader {...forwarded(rest)} render={render}>
      {children}
    </SheetHeader>
  );
}

/** The bottom of the drawer, where the actions go. */
export component DrawerFooter(children: React.Node, render?: RenderProp, ...rest: Rest) {
  return (
    <SheetFooter {...forwarded(rest)} render={render}>
      {children}
    </SheetFooter>
  );
}

/** The drawer's accessible name. */
export component DrawerTitle(children: React.Node, render?: RenderProp, ...rest: Rest) {
  return (
    <SheetTitle {...forwarded(rest)} render={render}>
      {children}
    </SheetTitle>
  );
}

/** What the drawer is for, announced after its name. */
export component DrawerDescription(children: React.Node, render?: RenderProp, ...rest: Rest) {
  return (
    <SheetDescription {...forwarded(rest)} render={render}>
      {children}
    </SheetDescription>
  );
}

/**
 * A button that closes the drawer, and the single-pointer alternative to the
 * drag.
 *
 * It registers itself so `Drawer.Body` can tell whether the gesture has one.
 */
export component DrawerClose(children: React.Node, render?: RenderProp, ...rest: Rest) {
  const drawer = useDrawer("Drawer.Close");
  const closes = drawer.closes;

  useEffect(() => {
    closes.current += 1;
    return () => {
      closes.current -= 1;
    };
  }, [closes]);

  return (
    <SheetClose {...forwarded(rest)} render={render}>
      {children}
    </SheetClose>
  );
}

/**
 * The grip: a slider over the snap points, and the thing the finger drags.
 *
 * Both halves are the same control on purpose. A drag handle that is not
 * focusable is the WCAG 2.1.1 failure; a pair of arrow buttons beside a drag
 * handle is two controls for one job, and a reader who found one has no way to
 * know the other exists. `role="slider"` says what it does — the snap points
 * are its values — and `Home` and `End` are the ends of the list.
 *
 * `label` because a slider with no accessible name is announced as "slider",
 * which is the same failure `Resizable.Handle` names.
 */
export component DrawerHandle(
  label?: string = "Resize the drawer",
  render?: RenderProp,
  ...rest: Rest
) {
  const drawer = useDrawer("Drawer.Handle");
  const { bodyRef, close, handles, setSnapIndex, side, snapIndex, snapPoints } = drawer;
  const passed = withoutComposed(rest, [
    "onKeyDown",
    "onPointerDown",
    "onPointerMove",
    "onPointerUp",
  ]);
  // Where the finger went down, and along which axis. A ref because nothing
  // renders it: it is a fact about a gesture in progress.
  const dragFrom = useRef<number | null>(null);
  const [dragging, setDragging] = useState(false);
  const vertical = side === "top" || side === "bottom";
  const last = snapPoints.length - 1;

  // So `Drawer.Body` knows there is a drag to provide an alternative to. A
  // drawer with no handle has no gesture, and requiring a close button of one
  // would be this component inventing a rule WCAG did not write.
  useEffect(() => {
    handles.current += 1;
    return () => {
      handles.current -= 1;
    };
  }, [handles]);

  /** Move by one snap point, or close when there is no smaller one. */
  const step = (towardsOpen: boolean) => {
    if (towardsOpen) {
      setSnapIndex(Math.min(last, snapIndex + 1));
      return;
    }
    if (snapIndex === 0) {
      // "Drag it off the edge", as a key. Without this the smallest snap point
      // is a floor the keyboard cannot get past and the gesture is the only
      // way to dismiss it.
      close();
      return;
    }
    setSnapIndex(snapIndex - 1);
  };

  /** How far a pointer has travelled towards closing the drawer, in pixels. */
  const travelled = (event: $FlowFixMe): number => {
    const from = dragFrom.current;
    if (from == null) {
      return 0;
    }
    const now = vertical ? event.clientY : event.clientX;
    // Closing is towards the edge the drawer is attached to, which is the
    // negative direction for a `top` or `left` drawer and the positive one for
    // the other two.
    return side === "top" || side === "left" ? from - now : now - from;
  };

  const props = withProps(passed, {
    "aria-label": label,
    // The axis the drag runs along, which is the axis the snap points are
    // measured on: a bottom sheet grows upwards, so its slider is vertical.
    "aria-orientation": vertical ? "vertical" : "horizontal",
    "aria-valuemax": last,
    "aria-valuemin": 0,
    "aria-valuenow": snapIndex,
    // The number a reader can act on. `aria-valuenow` is an index into a list
    // nobody outside this component has seen, and "2" says nothing.
    "aria-valuetext": `${String(Math.round((snapPoints[snapIndex] ?? 1) * 100))}%`,
    "data-dragging": dragging ? "true" : undefined,
    onKeyDown: composeHandlers(rest.onKeyDown, (event: $FlowFixMe) => {
      if (event.key === "Home" || event.key === "End") {
        event.preventDefault();
        setSnapIndex(event.key === "Home" ? 0 : last);
        return;
      }
      const towardsOpen = OPENS_WITH[side];
      const towardsClosed = CLOSES_WITH[side];
      if (event.key === towardsOpen) {
        event.preventDefault();
        step(true);
        return;
      }
      if (event.key === towardsClosed) {
        event.preventDefault();
        step(false);
      }
    }),
    onPointerDown: composeHandlers(rest.onPointerDown, (event: $FlowFixMe) => {
      dragFrom.current = vertical ? event.clientY : event.clientX;
      setDragging(true);
      // So the drag survives the pointer leaving the handle, which it does
      // immediately: the handle moves with the drawer.
      event.currentTarget?.setPointerCapture?.(event.pointerId);
    }),
    onPointerMove: composeHandlers(rest.onPointerMove, (event: $FlowFixMe) => {
      const body = bodyRef.current;
      if (dragFrom.current == null || body == null) {
        return;
      }
      // Only away from the edge: dragging a drawer *past* fully open would
      // otherwise lift it off the edge it is attached to.
      body.style.setProperty("--uf-drawer-drag", `${String(Math.max(0, travelled(event)))}px`);
    }),
    onPointerUp: composeHandlers(rest.onPointerUp, (event: $FlowFixMe) => {
      const body = bodyRef.current;
      const moved = travelled(event);
      dragFrom.current = null;
      setDragging(false);
      body?.style.setProperty("--uf-drawer-drag", "0px");
      if (body == null) {
        return;
      }
      const box = body.getBoundingClientRect();
      const size = vertical ? box.height : box.width;
      // A zero-sized box — a document that computes no layout — must not turn
      // every release into a dismissal.
      if (size <= 0 || Math.abs(moved) < size * DRAG_THRESHOLD) {
        return;
      }
      step(moved < 0);
    }),
    role: "slider",
    // A drag handle that is not in the tab sequence is the WCAG 2.1.1
    // failure this part exists to avoid.
    tabIndex: 0,
  });

  if (render != null) {
    return render(props);
  }
  return <div {...props} />;
}

/**
 * The key that makes the drawer bigger, per edge.
 *
 * A bottom sheet grows upwards and a left drawer grows to the right, so the
 * arrow that opens one closes another. Written as a table rather than a
 * conditional because there are four of them and the mistake to avoid is
 * getting one wrong.
 */
const OPENS_WITH: { readonly [Edge]: string } = {
  bottom: "ArrowUp",
  left: "ArrowRight",
  right: "ArrowLeft",
  top: "ArrowDown",
};

/** The key that makes it smaller, and closes it at the smallest snap point. */
const CLOSES_WITH: { readonly [Edge]: string } = {
  bottom: "ArrowDown",
  left: "ArrowLeft",
  right: "ArrowRight",
  top: "ArrowUp",
};
