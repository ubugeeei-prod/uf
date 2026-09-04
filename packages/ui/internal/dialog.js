// @flow
//
// A modal dialog, which is the component people most often get wrong.
//
// Four things have to be true for a dialog to be usable by someone who is not
// using a mouse, and a hand-written one usually has one or two of them:
//
//   * Focus moves into the dialog when it opens, and to the first thing worth
//     acting on rather than to whatever happens to be first in the document.
//   * Tab cannot leave. A dialog you can Tab out of leaves the reader
//     somewhere in a page they cannot see, with no way back.
//   * Escape closes it.
//   * Focus returns to whatever opened it. Otherwise it restarts at the top of
//     the document, and the reader has to find their place again.
//
// The dialog is rendered where it is declared rather than through a portal.
// A portal solves a stacking-context problem that belongs to CSS, and it costs
// the thing this component is for: rendered in place, the dialog is next to its
// trigger in the accessibility tree, which is where a screen reader looks.

import * as React from "@uniflowed/react";

import { composeHandlers, composeRefs, withoutComposed } from "./props.js";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "@uniflowed/react";

type DialogState = {|
  readonly base: string,
  readonly open: boolean,
  readonly setOpen: (open: boolean) => void,
  readonly triggerRef: { current: HTMLElement | null },
|};

const DialogContext: React.Context<DialogState | null> = createContext(null);

function useDialog(part: string): DialogState {
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
  const [internal, setInternal] = useState(defaultOpen);
  const triggerRef = useRef<HTMLElement | null>(null);
  const isOpen = open ?? internal;

  const setOpen = useCallback(
    (next: boolean) => {
      if (open == null) {
        setInternal(next);
      }
      onOpenChange?.(next);
    },
    [open, onOpenChange],
  );

  const state = useMemo(
    () => ({ base, open: isOpen, setOpen, triggerRef }),
    [base, isOpen, setOpen],
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
 * The dialog itself: focus moved in, Tab kept inside, Escape closing it.
 *
 * `aria-modal` tells a screen reader that the rest of the page is not
 * available, which is the half of "modal" that CSS cannot express.
 */
export component DialogContent(children: React.Node, ...rest: { readonly [string]: mixed }) {
  const dialog = useDialog("Dialog.Content");
  const contentRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!dialog.open) {
      return;
    }
    const opener = dialog.triggerRef.current;
    const content = contentRef.current;
    // The first thing worth acting on, not the first thing in the document —
    // and the dialog itself if it contains nothing focusable, so focus is
    // inside it either way.
    const target = content == null ? null : focusable(content)[0] ?? content;
    target?.focus();

    return () => {
      // Back to the trigger. Leaving focus on a removed node sends it to the
      // top of the document, and the reader has to find their place again.
      opener?.focus();
    };
  }, [dialog.open, dialog.triggerRef]);

  if (!dialog.open) {
    return null;
  }

  const passed = withoutComposed(rest, ["onKeyDown", "ref"]);

  return (
    <div
      // `passed` first. A caller `ref` used to replace `contentRef`, which
      // left it null, made the Tab branch below return early, and turned the
      // focus trap off while the dialog still announced `aria-modal="true"`.
      // A caller `onKeyDown` used to replace this one, and Escape stopped
      // closing the dialog.
      {...passed}
      aria-labelledby={`${dialog.base}-title`}
      aria-modal="true"
      id={`${dialog.base}-content`}
      onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          dialog.setOpen(false);
          return;
        }
        if (event.key !== "Tab") {
          return;
        }
        const content = contentRef.current;
        if (content == null) {
          return;
        }
        const stops = focusable(content);
        if (stops.length === 0) {
          // Nothing to move to, so Tab must not leave either.
          event.preventDefault();
          return;
        }
        const first = stops[0];
        const last = stops[stops.length - 1];
        const active = content.ownerDocument?.activeElement;
        // Wrap at the ends. This is the whole of "focus cannot leave": every
        // other Tab press is the browser's own business.
        if (event.shiftKey && (active === first || active === content)) {
          event.preventDefault();
          last.focus();
        } else if (!event.shiftKey && active === last) {
          event.preventDefault();
          first.focus();
        }
      })}
      ref={composeRefs(rest.ref, (element) => {
        contentRef.current = element;
      })}
      role="dialog"
      tabIndex={-1}
    >
      {children}
    </div>
  );
}

/** The dialog's accessible name, which `aria-labelledby` points at. */
export component DialogTitle(children: React.Node, ...rest: { readonly [string]: mixed }) {
  const dialog = useDialog("Dialog.Title");
  return (
    <h2 {...rest} id={`${dialog.base}-title`}>
      {children}
    </h2>
  );
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
 * excludes them, and anything inside `[hidden]` or `aria-hidden` is excluded
 * because a reader cannot see it.
 */
function focusable(root: HTMLElement): Array<HTMLElement> {
  const selector =
    'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';
  return Array.from(root.querySelectorAll(selector)).filter(
    (element: any) =>
      // Both attributes hide a whole subtree, so both are checked on the
      // ancestors. Reading `aria-hidden` off the element alone returned a
      // button inside `<div aria-hidden="true">` as a focus stop, and the trap
      // then moved focus to a control no screen reader exposes.
      element.closest("[hidden]") == null && element.closest('[aria-hidden="true"]') == null,
  );
}
