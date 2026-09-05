// @flow
//
// A modal dialog, which is the component people most often get wrong.
//
// "Modal" is a promise made to somebody who cannot see the page. Dimming the
// background makes the promise to everyone else; these are the parts that make
// it to a reader using a keyboard and a screen reader, and a dialog that skips
// any one of them is a trap:
//
//   * **Focus moves in**, to the first thing worth acting on rather than to
//     whatever happens to be first in the document.
//   * **Tab cannot leave.** A dialog you can Tab out of leaves the reader
//     somewhere in a page they cannot see, with no way back in.
//   * **Escape closes it**, and closes *this* one rather than the one behind
//     it when two are stacked.
//   * **Focus returns to whatever opened it.** Otherwise focus falls to
//     `<body>`, the next Tab starts at the top of the page, and the reader has
//     to find their place again — which is the single most common complaint
//     about hand-written dialogs.
//   * **The rest of the page is gone**, not merely covered. `aria-modal` says
//     so to a screen reader and `inert` says so to the browser; a dimmed
//     backdrop says it only to people who can see the dim.
//   * **The page behind does not scroll**, because a wheel over a modal that
//     scrolls the document loses the reader's position in it.
//
// # Why it is not rendered through a portal
//
// A portal solves a stacking-context problem that belongs to CSS, and it costs
// the thing this component is for: rendered where it is written, the dialog is
// next to its trigger in the accessibility tree, which is where a screen reader
// looks. The page behind is hidden by marking it inert rather than by moving
// the dialog out of it, which gets the same guarantee without the move.
//
// # Composition
//
// The parts are one namespace — `Dialog.Root`, `Dialog.Body`, `Dialog.Title` —
// because they only work together: `Body` cannot label itself without `Title`,
// and `Title` has nothing to label without `Body`. See `index.js`.

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
} from "@uniflowed/react";
import { useStableCallback } from "@uniflowed/hooks/lifecycle";

import { composeHandlers, composeRefs, withoutComposed } from "./internal/merge-props.js";
import { useControlled } from "./internal/controlled-state.js";

type DialogState = {|
  readonly base: string,
  readonly open: boolean,
  readonly setOpen: (open: boolean) => void,
  readonly triggerRef: { current: HTMLElement | null },
  /** Whether a `Dialog.Title` is rendered, so `aria-labelledby` names one. */
  readonly titled: boolean,
  /** Whether a `Dialog.Description` is rendered. */
  readonly described: boolean,
  readonly registerTitle: (present: boolean) => void,
  readonly registerDescription: (present: boolean) => void,
|};

const DialogContext: React.Context<DialogState | null> = createContext(null);

/**
 * The dialog a part belongs to.
 *
 * Raising rather than returning null: a `Dialog.Title` outside a `Dialog.Root`
 * would render a heading with an id nothing points at, and would look correct.
 */
hook useDialog(part: string): DialogState {
  const state = useContext(DialogContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Dialog.Root`);
  }
  return state;
}

/** The dialog, open or closed. Uncontrolled unless `open` is given. */
export component DialogRoot(
  children: React.Node,
  defaultOpen?: boolean = false,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
) {
  const base = useId();
  const [isOpen, setOpen] = useControlled(open, defaultOpen, onOpenChange);
  const triggerRef = useRef<HTMLElement | null>(null);
  const [titled, setTitled] = useState(false);
  const [described, setDescribed] = useState(false);

  const state = useMemo(
    () => ({
      base,
      open: isOpen,
      setOpen,
      triggerRef,
      titled,
      described,
      registerTitle: setTitled,
      registerDescription: setDescribed,
    }),
    [base, isOpen, setOpen, titled, described],
  );

  return <DialogContext.Provider value={state}>{children}</DialogContext.Provider>;
}

/** What opens the dialog, and what focus comes back to when it closes. */
export component DialogTrigger(children: React.Node, ...rest: { readonly [string]: mixed }) {
  const dialog = useDialog("Dialog.Trigger");
  const passed = withoutComposed(rest, ["onClick", "ref"]);

  return (
    <button
      {...passed}
      // Only while it is open. An `aria-controls` naming an element that is not
      // in the document is worse than no `aria-controls`: a reader is told
      // there is somewhere to go and there is not.
      aria-controls={dialog.open ? `${dialog.base}-body` : undefined}
      aria-expanded={dialog.open ? "true" : "false"}
      aria-haspopup="dialog"
      onClick={composeHandlers(rest.onClick, () => dialog.setOpen(true))}
      ref={composeRefs(rest.ref, (element) => {
        dialog.triggerRef.current = element;
      })}
      type="button"
    >
      {children}
    </button>
  );
}

/**
 * The backdrop.
 *
 * Deliberately does nothing but exist and stay out of the accessibility tree:
 * it is `aria-hidden` because a reader has no use for a rectangle, and it does
 * *not* own the close-on-outside-press behaviour, because a caller who styles
 * their own backdrop or omits one entirely must still get it. That lives on
 * `Dialog.Body`, which is the part that knows where "outside" is.
 */
export component DialogOverlay(...rest: { readonly [string]: mixed }) {
  const dialog = useDialog("Dialog.Overlay");
  if (!dialog.open) {
    return null;
  }
  return <div {...rest} aria-hidden="true" data-state="open" />;
}

/**
 * The dialog itself: focus moved in, kept in, and given back.
 *
 * `aria-modal` tells a screen reader that the rest of the page is unavailable,
 * which is the half of "modal" that CSS cannot express; `inert` on everything
 * outside is the half the browser enforces.
 */
export component DialogBody(children: React.Node, ...rest: { readonly [string]: mixed }) {
  const dialog = useDialog("Dialog.Body");
  const bodyRef = useRef<HTMLElement | null>(null);
  // Stable, so the effect below depends on `open` and on nothing else. Keyed on
  // `setOpen` it re-ran whenever the caller passed a fresh `onOpenChange`
  // closure — which is every render — and re-running it re-took focus, so a
  // parent that re-rendered stole focus back from whatever the reader had
  // moved it to inside the dialog.
  const close = useStableCallback(() => dialog.setOpen(false));

  useEffect(() => {
    const body = bodyRef.current;
    if (!dialog.open || body == null) {
      return;
    }
    const document = body.ownerDocument;
    const trigger = dialog.triggerRef.current;
    // Whatever had focus, which is the trigger for a dialog that was opened
    // and the previously focused element for one that opened itself.
    const opener = trigger ?? (document.activeElement as $FlowFixMe);

    const restorePage = concealOutside(body);
    const releaseScroll = lockScroll(document);

    const onOutsidePress = (event: Event) => {
      const target: $FlowFixMe = event.target;
      if (target == null || body.contains(target)) {
        return;
      }
      // The trigger is outside the dialog and is not "outside" for this
      // purpose: closing here and letting the trigger's own click reopen made
      // a press on the trigger a no-op that flickered.
      if (trigger != null && trigger.contains(target)) {
        return;
      }
      close();
    };
    // Capture, so a press is seen even where something below it stops the
    // event — a menu inside the dialog, for instance.
    document.addEventListener("pointerdown", onOutsidePress, true);

    // The first thing worth acting on, and the dialog itself when it holds
    // nothing focusable, so focus is inside it either way.
    const target = focusable(body)[0] ?? body;
    target.focus();

    return () => {
      document.removeEventListener("pointerdown", onOutsidePress, true);
      // Order matters: the page comes back before focus is restored, because
      // the trigger is one of the elements that was made `inert` and an inert
      // element cannot take focus.
      restorePage();
      releaseScroll();
      opener?.focus?.();
    };
  }, [dialog.open, dialog.triggerRef, close]);

  if (!dialog.open) {
    return null;
  }

  const passed = withoutComposed(rest, ["onKeyDown", "ref"]);

  return (
    <div
      // `passed` first. A caller `ref` used to replace `bodyRef`, which left it
      // null, made the Tab branch below return early, and turned the focus trap
      // off while the dialog still announced `aria-modal="true"`. A caller
      // `onKeyDown` used to replace this one, and Escape stopped closing it.
      {...passed}
      // Only ids that are in the document: an `aria-labelledby` naming a
      // missing element makes a screen reader announce nothing at all, so a
      // dialog without a `Dialog.Title` falls through to whatever `aria-label`
      // the caller passed instead.
      aria-describedby={dialog.described ? `${dialog.base}-description` : undefined}
      aria-labelledby={dialog.titled ? `${dialog.base}-title` : undefined}
      aria-modal="true"
      id={`${dialog.base}-body`}
      onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          // The dialog behind this one must not also close. Two stacked
          // dialogs nest in the DOM, so without this the event bubbled to the
          // outer dialog's handler and one Escape closed both.
          event.stopPropagation();
          close();
          return;
        }
        if (event.key !== "Tab") {
          return;
        }
        const body = bodyRef.current;
        if (body == null) {
          return;
        }
        const stops = focusable(body);
        // An outer dialog must not also run its trap on this key.
        event.stopPropagation();
        if (stops.length === 0) {
          // Nothing to move to, so Tab must not leave either.
          event.preventDefault();
          return;
        }
        const first = stops[0];
        const last = stops[stops.length - 1];
        const active = body.ownerDocument?.activeElement;
        // Wrap at the ends. This is the whole of "focus cannot leave"; every
        // other Tab press is the browser's own business.
        if (event.shiftKey && (active === first || active === body)) {
          event.preventDefault();
          last.focus();
        } else if (!event.shiftKey && active === last) {
          event.preventDefault();
          first.focus();
        }
      })}
      ref={composeRefs(rest.ref, (element) => {
        bodyRef.current = element;
      })}
      role="dialog"
      // So the dialog can hold focus itself when it contains nothing focusable,
      // and so the trap has somewhere to put focus that is still inside.
      tabIndex={-1}
    >
      {children}
    </div>
  );
}

/**
 * The dialog's accessible name, which `aria-labelledby` points at.
 *
 * It registers itself so `Dialog.Body` only claims a name when one is actually
 * rendered — a conditional title that is absent used to leave the dialog
 * pointing at an id nothing had.
 */
export component DialogTitle(children: React.Node, ...rest: { readonly [string]: mixed }) {
  const dialog = useDialog("Dialog.Title");
  const register = dialog.registerTitle;
  useEffect(() => {
    register(true);
    return () => register(false);
  }, [register]);

  return (
    <h2 {...rest} id={`${dialog.base}-title`}>
      {children}
    </h2>
  );
}

/**
 * What the dialog is for, announced after its name.
 *
 * A screen reader reads the description when focus enters the dialog, which is
 * the one moment the reader has to decide whether they care — so this is where
 * "this cannot be undone" belongs, not in body text further down.
 */
export component DialogDescription(children: React.Node, ...rest: { readonly [string]: mixed }) {
  const dialog = useDialog("Dialog.Description");
  const register = dialog.registerDescription;
  useEffect(() => {
    register(true);
    return () => register(false);
  }, [register]);

  return (
    <p {...rest} id={`${dialog.base}-description`}>
      {children}
    </p>
  );
}

/**
 * The top of the dialog, as a place to put styles.
 *
 * A `<div>` rather than a `<header>` on purpose: a `<header>` is a `banner`
 * landmark, and a second banner inside a dialog is a landmark a reader will
 * find in the landmark list and be unable to explain. The part exists so the
 * styling layer has a name to attach to, and contributes no semantics because
 * it has none to contribute.
 */
export component DialogHeader(children: React.Node, ...rest: { readonly [string]: mixed }) {
  return <div {...rest}>{children}</div>;
}

/** The bottom of the dialog, where the actions go. See `Dialog.Header`. */
export component DialogFooter(children: React.Node, ...rest: { readonly [string]: mixed }) {
  return <div {...rest}>{children}</div>;
}

/** A button that closes the dialog. */
export component DialogClose(children: React.Node, ...rest: { readonly [string]: mixed }) {
  const dialog = useDialog("Dialog.Close");
  const passed = withoutComposed(rest, ["onClick"]);

  return (
    <button
      {...passed}
      onClick={composeHandlers(rest.onClick, () => dialog.setOpen(false))}
      type="button"
    >
      {children}
    </button>
  );
}

/**
 * The focus stops inside an element, in document order.
 *
 * Disabled controls and `tabindex="-1"` are excluded because the browser
 * excludes them, and anything inside `[hidden]`, `[inert]` or `aria-hidden` is
 * excluded because a reader cannot reach it.
 */
function focusable(root: HTMLElement): Array<HTMLElement> {
  const selector =
    'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';
  return Array.from(root.querySelectorAll(selector)).filter(
    (element: $FlowFixMe) =>
      // All three attributes hide a whole subtree, so all three are checked on
      // the ancestors. Reading `aria-hidden` off the element alone returned a
      // button inside `<div aria-hidden="true">` as a focus stop, and the trap
      // then moved focus to a control no screen reader exposes.
      element.closest("[hidden]") == null &&
      element.closest("[inert]") == null &&
      element.closest('[aria-hidden="true"]') == null,
  );
}

/**
 * Take everything outside `element` out of the page, and give it back.
 *
 * Walking up from the dialog and hiding each level's *siblings*, rather than
 * hiding the top-level children of `<body>`, because that is what makes two
 * stacked dialogs work: the inner one is inside the outer one's subtree, so
 * hiding body's children would hide nothing new and the outer dialog's own
 * content would stay readable behind the inner one.
 *
 * Both attributes, because they address different audiences. `aria-hidden`
 * removes the subtree from the accessibility tree; `inert` also stops clicks
 * and takes it out of the tab order, which is the browser's own enforcement of
 * the focus trap and does not depend on this component's key handling being
 * reached.
 */
function concealOutside(element: HTMLElement): () => void {
  const document = element.ownerDocument;
  const restore: Array<{| element: Element, hidden: string | null, inert: boolean |}> = [];

  let node: Element | null = element;
  while (node != null && node !== document.body) {
    const parent = node.parentElement;
    if (parent == null) {
      break;
    }
    for (const sibling of Array.from(parent.children)) {
      if (sibling === node) {
        continue;
      }
      restore.push({
        element: sibling,
        hidden: sibling.getAttribute("aria-hidden"),
        inert: sibling.hasAttribute("inert"),
      });
      sibling.setAttribute("aria-hidden", "true");
      sibling.setAttribute("inert", "");
    }
    node = parent;
  }

  return () => {
    // In reverse, so an element concealed by two nested dialogs is handed back
    // the state the outer one found rather than the state the inner one did.
    for (let index = restore.length - 1; index >= 0; index -= 1) {
      const entry = restore[index];
      if (entry.hidden == null) {
        entry.element.removeAttribute("aria-hidden");
      } else {
        entry.element.setAttribute("aria-hidden", entry.hidden);
      }
      if (!entry.inert) {
        entry.element.removeAttribute("inert");
      }
    }
  };
}

/**
 * How many dialogs are holding the page still, and what it looked like before.
 *
 * A count rather than each dialog saving and restoring, because two dialogs
 * that open and close in any order but the strictest nesting would otherwise
 * hand the page back a value the other one had already replaced.
 */
let scrollLocks = 0;
let overflowBeforeLock: string = "";

/** Stop the page behind the dialog from scrolling, and undo exactly that. */
function lockScroll(document: Document): () => void {
  const body: $FlowFixMe = document.body;
  if (scrollLocks === 0) {
    overflowBeforeLock = body.style.overflow;
    body.style.overflow = "hidden";
  }
  scrollLocks += 1;

  let released = false;
  return () => {
    if (released) {
      return;
    }
    released = true;
    scrollLocks -= 1;
    if (scrollLocks === 0) {
      body.style.overflow = overflowBeforeLock;
      overflowBeforeLock = "";
    }
  };
}
