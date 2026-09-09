// @flow
//
// An alert dialog: the modal a reader has to answer.
//
// It is `dialog.js` with the three decisions that module's header names taken
// the other way, and it is a component rather than a page of advice because
// each of the three is silent when it is wrong:
//
//   * **`role="alertdialog"`.** A screen reader announces an `alertdialog`'s
//     description as soon as focus arrives, without waiting to be asked. That
//     is the whole of what the role buys, and it is why the description below
//     is not optional.
//   * **A press outside does not close it.** There is no way to decline by
//     accident. `Escape` still closes it, because a modal a reader cannot leave
//     from the keyboard is a trap and declining is what `Escape` means — so the
//     two dismissals differ deliberately: the deliberate one works, the
//     accidental one does not.
//   * **Focus lands on `AlertDialog.Cancel`.** The APG puts it on the least
//     destructive action, and `Cancel` is that action by construction here
//     rather than by a caller remembering to pass a ref. A confirmation whose
//     `Enter` deletes the project is a confirmation that asked nothing.
//
// # Why the description is required
//
// `aria-describedby` is what makes `alertdialog` worth using. An alert dialog
// with nothing to announce is a `dialog` that has told the reader's software to
// expect something urgent and then said only its title — which is worse than
// the plain `Dialog`, because the reader has been interrupted for nothing.
//
// So `AlertDialog.Body` raises when no `AlertDialog.Description` is inside it.
// Raising rather than warning, for the reason `useDialog` gives: the failure is
// invisible in the markup, invisible in a screenshot, and audible only to
// somebody who is not in the room. A component that lets it through ships it.
//
// # Action and Cancel are two parts, not one `Close` with a variant
//
// `Dialog.Close` closes the dialog and says nothing about what closing meant.
// The two buttons of a confirmation mean opposite things — one carries out the
// thing being confirmed, the other declines it — and a reader who has been
// asked a question is entitled to have the answer be a named button rather
// than a `variant="destructive"` on a shared one. `Cancel` is also the part
// focus goes to, which is a behaviour a variant cannot carry.

"use client";

import * as React from "@uniflowed/react";
import { createContext, useContext, useEffect, useMemo, useRef } from "@uniflowed/react";

import type { RenderProp, Rest } from "./internal/merge-props.js";
import { composeRefs, forwarded, withoutComposed } from "./internal/merge-props.js";
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

type AlertDialogState = {|
  /**
   * The least destructive action, and where focus goes.
   *
   * A ref rather than state, because nothing renders it: it is read once, by
   * `Dialog.Body`'s focus effect, after the commit that attached it.
   */
  readonly cancelRef: { current: HTMLElement | null },
  /**
   * How many `AlertDialog.Description`s are in the document.
   *
   * A counted ref rather than the `described` boolean `Dialog.Root` already
   * keeps, because that one is state: it is `false` on the commit that mounts
   * the description, so a check against it would raise on every alert dialog
   * ever rendered. A child's effect runs before its parent's, so by the time
   * `AlertDialog.Body` asks, every description below it has answered.
   */
  readonly describedBy: { current: number },
|};

const AlertDialogContext: React.Context<AlertDialogState | null> = createContext(null);

/**
 * The alert dialog a part belongs to.
 *
 * Raising rather than returning null, for the reason `useDialog` gives: an
 * `AlertDialog.Cancel` outside a root would render a button that closes nothing
 * and takes no focus, and it would look correct.
 */
hook useAlertDialog(part: string): AlertDialogState {
  const state = useContext(AlertDialogContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside an AlertDialog.Root`);
  }
  return state;
}

/** The alert dialog, open or closed. Uncontrolled unless `open` is given. */
export component AlertDialogRoot(
  children: React.Node,
  defaultOpen?: boolean = false,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
) {
  const cancelRef = useRef<HTMLElement | null>(null);
  const describedBy = useRef(0);
  const state = useMemo(() => ({ cancelRef, describedBy }), []);

  return (
    <AlertDialogContext.Provider value={state}>
      <DialogRoot defaultOpen={defaultOpen} onOpenChange={onOpenChange} open={open}>
        {children}
      </DialogRoot>
    </AlertDialogContext.Provider>
  );
}

/** What opens it, and what focus comes back to when it closes. */
export component AlertDialogTrigger(children: React.Node, render?: RenderProp, ...rest: Rest) {
  return (
    <DialogTrigger {...forwarded(rest)} render={render}>
      {children}
    </DialogTrigger>
  );
}

/** The backdrop. See `Dialog.Overlay`: it is decoration and says so. */
export component AlertDialogOverlay(render?: RenderProp, ...rest: Rest) {
  return <DialogOverlay {...forwarded(rest)} render={render} />;
}

/**
 * The alert dialog itself: announced as one, described, and not dismissible by
 * a press beside it.
 *
 * The three props it sets on `Dialog.Body` are the three the module header
 * names, and they are set here rather than left to a caller because a caller
 * who set two of them would have an alert dialog that is wrong in the third
 * without anything saying so.
 */
export component AlertDialogBody(children: React.Node, render?: RenderProp, ...rest: Rest) {
  const alert = useAlertDialog("AlertDialog.Body");

  return (
    <DialogBody
      {...forwarded(rest)}
      dismissOnOutsidePress={false}
      initialFocus={alert.cancelRef}
      render={render}
      role="alertdialog"
    >
      {children}
      <RequireDescription />
    </DialogBody>
  );
}

/**
 * The check that there is something to announce, made where it is answerable.
 *
 * Inside `Dialog.Body` and last, and both halves are load-bearing. Inside,
 * because `Dialog.Body` renders nothing at all while it is closed — an alert
 * dialog that has not been opened has no description in the document, and a
 * check in `AlertDialog.Body` itself therefore fired on every alert dialog ever
 * rendered. Last, because React runs a subtree's effects in document order, so
 * every `AlertDialog.Description` above this has already counted itself by the
 * time this asks.
 *
 * It renders nothing, which is the point: the requirement is about the tree and
 * not about the markup.
 */
component RequireDescription() {
  const alert = useAlertDialog("AlertDialog.Body");
  const describedBy = alert.describedBy;

  useEffect(() => {
    if (describedBy.current === 0) {
      throw new Error(
        "AlertDialog.Body must contain an AlertDialog.Description: " +
          'role="alertdialog" exists to announce one, and an alert dialog ' +
          "without a description interrupts the reader to say nothing.",
      );
    }
  }, [describedBy]);

  return null;
}

/** The top of the alert dialog. See `Dialog.Header` for why it is a `div`. */
export component AlertDialogHeader(children: React.Node, render?: RenderProp, ...rest: Rest) {
  return (
    <DialogHeader {...forwarded(rest)} render={render}>
      {children}
    </DialogHeader>
  );
}

/** The bottom, where `Action` and `Cancel` go. */
export component AlertDialogFooter(children: React.Node, render?: RenderProp, ...rest: Rest) {
  return (
    <DialogFooter {...forwarded(rest)} render={render}>
      {children}
    </DialogFooter>
  );
}

/** The question, which is the alert dialog's accessible name. */
export component AlertDialogTitle(children: React.Node, render?: RenderProp, ...rest: Rest) {
  return (
    <DialogTitle {...forwarded(rest)} render={render}>
      {children}
    </DialogTitle>
  );
}

/**
 * What answering costs, announced the moment focus arrives.
 *
 * This is where "this cannot be undone" belongs. It is the sentence the role
 * exists to deliver, and the only moment the reader has to decide whether they
 * care is before they have pressed anything.
 */
export component AlertDialogDescription(children: React.Node, render?: RenderProp, ...rest: Rest) {
  const alert = useAlertDialog("AlertDialog.Description");
  const describedBy = alert.describedBy;

  useEffect(() => {
    describedBy.current += 1;
    return () => {
      describedBy.current -= 1;
    };
  }, [describedBy]);

  return (
    <DialogDescription {...forwarded(rest)} render={render}>
      {children}
    </DialogDescription>
  );
}

/**
 * The button that carries out the thing being confirmed.
 *
 * It closes the dialog after the caller's handler has run, and it is not where
 * focus starts; see `AlertDialog.Cancel`. `Dialog.Close` is what both answers
 * are made of, so the composition rule about a caller's `onClick` has one
 * implementation rather than a second copy here.
 */
export component AlertDialogAction(children: React.Node, render?: RenderProp, ...rest: Rest) {
  return (
    <DialogClose {...forwarded(rest)} render={render}>
      {children}
    </DialogClose>
  );
}

/**
 * The button that declines, and the one focus lands on.
 *
 * It registers itself so `AlertDialog.Body` can name it as the initial focus
 * without the caller wiring a ref: the least destructive action is a fact about
 * which part this is, not a decision to be repeated at every call.
 */
export component AlertDialogCancel(children: React.Node, render?: RenderProp, ...rest: Rest) {
  const alert = useAlertDialog("AlertDialog.Cancel");
  const cancelRef = alert.cancelRef;
  const passed = withoutComposed(rest, ["ref"]);

  return (
    <DialogClose
      {...forwarded(passed)}
      ref={composeRefs(rest.ref, (element: HTMLElement | null) => {
        cancelRef.current = element;
      })}
      render={render}
    >
      {children}
    </DialogClose>
  );
}
