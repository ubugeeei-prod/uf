// @flow
//
// A sheet: a modal dialog attached to an edge of the viewport.
//
// # What it is not, first
//
// A `Sheet` that were only a `Dialog` with a class on it would not be worth
// shipping, and this module would be a paragraph in the documentation saying
// "use `Dialog` and style it". The edge is a visual decision, `role="dialog"`
// is already right, and nothing a screen reader is told changes because the
// dialog slid in from the left. A component that added a `<div>` and a name
// for that would be a component that made a design system harder to read.
//
// What makes it a component is the two things a class cannot be:
//
//   * **`side` is a type.** `<Sheet.Root side="lft">` is a Flow error at the
//     call. A class name is a string, and a misspelt one is a sheet rendered
//     off the top of the page with nothing reported anywhere.
//   * **`data-side` is one contract.** The same attribute name `popover.js`
//     writes, so a stylesheet has one thing to key on for every overlay in this
//     package — and, more to the point, `drawer.js` and `sidebar.js` are both
//     *defined in terms of this module*. A drawer is a sheet you can drag away;
//     a sidebar on a narrow viewport becomes one. Written three times, "left"
//     would come to mean three subtly different things, and the day one of them
//     was fixed is the day they stopped agreeing.
//
// So the module is small on purpose. It is the shared definition of an edge,
// and the modal semantics under it are `dialog.js`'s, unchanged: focus in,
// `Tab` trapped, `Escape` out, focus back, the page inert and still. A sheet is
// modal, and every one of those is why.
//
// # The edge is not announced
//
// Deliberately. There is no `aria-*` for "this came in from the right", and
// inventing one — a `Sheet` that named its edge in its accessible name — would
// make a reader hear "Filters, right" and wonder what "right" meant. Where the
// box came from is the eye's business. The reader is told it is a dialog, what
// it is called, and that the rest of the page is unavailable, which is all
// three of the things that are true.

"use client";

import * as React from "@uniflowed/react";
import { createContext, useContext, useMemo } from "@uniflowed/react";

import type { Rest } from "./internal/merge-props.js";
import { forwarded } from "./internal/merge-props.js";
import {
  DialogBody,
  DialogClose,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
  DialogRoot,
  DialogTitle,
  DialogTrigger,
} from "./dialog.js";

/**
 * Which edge of the viewport a sheet is attached to.
 *
 * Physical, and deliberately not logical: a design that puts a navigation sheet
 * against the left of the screen means the left of the screen in any writing
 * direction, the same way `internal/anchor.js`'s `Side` does and for the same
 * reason. What the writing direction changes is the reading order inside the
 * sheet, which is the page's business rather than this component's.
 *
 * A union rather than a string, so a typo is a type error at the call rather
 * than a `data-side="lft"` no stylesheet matches and no test notices.
 */
export type Edge = "top" | "right" | "bottom" | "left";

type SheetState = {| readonly side: Edge |};

const SheetContext: React.Context<SheetState | null> = createContext(null);

/**
 * The sheet a part belongs to.
 *
 * Raising rather than returning null, for the reason `useDialog` gives: a
 * `Sheet.Body` outside a root would render a dialog with no edge, and it would
 * look correct until somebody styled it.
 */
hook useSheet(part: string): SheetState {
  const state = useContext(SheetContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Sheet.Root`);
  }
  return state;
}

/** The sheet, open or closed. Uncontrolled unless `open` is given. */
export component SheetRoot(
  children: React.Node,
  defaultOpen?: boolean = false,
  onOpenChange?: (open: boolean) => void,
  open?: boolean,
  side?: Edge = "right",
) {
  const state = useMemo(() => ({ side }), [side]);

  return (
    <SheetContext.Provider value={state}>
      <DialogRoot defaultOpen={defaultOpen} onOpenChange={onOpenChange} open={open}>
        {children}
      </DialogRoot>
    </SheetContext.Provider>
  );
}

/** What opens it, and what focus comes back to when it closes. */
export component SheetTrigger(children: React.Node, ...rest: Rest) {
  return <DialogTrigger {...forwarded(rest)}>{children}</DialogTrigger>;
}

/**
 * The backdrop, which knows the edge so a stylesheet does not have to be told
 * twice.
 */
export component SheetOverlay(...rest: Rest) {
  const sheet = useSheet("Sheet.Overlay");
  return <DialogOverlay {...forwarded(rest)} data-side={sheet.side} />;
}

/**
 * The sheet itself: `Dialog.Body`, plus the edge as an attribute.
 *
 * Every modal promise `dialog.js` makes is made here, unchanged. This part adds
 * `data-side` and nothing else, which is the honest size of the difference.
 */
export component SheetBody(children: React.Node, ...rest: Rest) {
  const sheet = useSheet("Sheet.Body");

  return (
    <DialogBody {...forwarded(rest)} data-side={sheet.side}>
      {children}
    </DialogBody>
  );
}

/** The top of the sheet. See `Dialog.Header` for why it is not a `<header>`. */
export component SheetHeader(children: React.Node, ...rest: Rest) {
  return <DialogHeader {...forwarded(rest)}>{children}</DialogHeader>;
}

/** The bottom of the sheet, where the actions go. */
export component SheetFooter(children: React.Node, ...rest: Rest) {
  return <DialogFooter {...forwarded(rest)}>{children}</DialogFooter>;
}

/** The sheet's accessible name. A modal without one is announced as "dialog". */
export component SheetTitle(children: React.Node, ...rest: Rest) {
  return <DialogTitle {...forwarded(rest)}>{children}</DialogTitle>;
}

/** What the sheet is for, announced after its name. */
export component SheetDescription(children: React.Node, ...rest: Rest) {
  return <DialogDescription {...forwarded(rest)}>{children}</DialogDescription>;
}

/** A button that closes the sheet. */
export component SheetClose(children: React.Node, ...rest: Rest) {
  return <DialogClose {...forwarded(rest)}>{children}</DialogClose>;
}
