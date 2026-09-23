"use client";
// @flow
//
// Dialog: `@uniflowed/ui`'s modal dialog, dressed in uf's tokens.
//
// `uf ui add dialog` wrote this file into the project, and it is the project's
// from then on. `uf ui diff dialog` shows how it has moved away from the
// registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The look: the scrim, the panel, the spacing, the close button in the corner.
// The behaviour is imported from `@uniflowed/ui`'s `Dialog`, and staying imported
// is the point of the arrangement. Focus moved into the dialog and kept there,
// `Escape`, the press outside, focus given back to whatever opened it, the page
// behind made inert and held still: each of those is a fix that reaches this
// project by upgrading `@uniflowed/ui`, which a copy of the behaviour would
// never receive. Editing this file changes how the dialog looks. It cannot
// quietly unmake what the dialog does.
//
// # What to keep true when you change it
//
// * **A dialog needs a name.** `Dialog.Title` is what `aria-labelledby` points
//   at, and a dialog without one is announced as "dialog" and nothing else. A
//   design with no visible title passes `aria-label` to `Dialog.Content`.
// * **The description is read when focus arrives.** "This cannot be undone"
//   belongs in `Dialog.Description`, which a screen reader reads with the name,
//   rather than in body text further down.
// * **The close button is last in the markup and first on the screen.** Focus
//   moves to the first thing worth acting on when the dialog opens, and that
//   should be the task rather than the corner `×`; the button is put in the
//   corner by position, so the order a reader tabs through is still the order
//   that makes sense. Its name is `closeLabel`, which is "Close" until you
//   translate it.
// * **The panel scrolls and the page does not.** It is capped at the viewport
//   less a margin and scrolls inside itself, so a long form stays reachable on
//   a phone held sideways.
// * **Colour comes from tokens, in measured pairs.** `ink` and `muted` on
//   `surface`, which `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in
//   the light default and the dark theme.
// * **It enters, and does not yet leave.** The scrim fades in and the panel
//   fades in from 96% of its size, over `durationSlow`, from a
//   `@starting-style` — a transition from the style the element is taken to
//   have had before it was inserted, which needs neither `@keyframes` (uf's
//   StyleX has none) nor the panel mounted while closed. Leaving is a cut for
//   now: an exit transition needs `@uniflowed/ui` to keep the closing panel
//   mounted until it finishes, which it does not do yet. Under reduced motion
//   the panel only fades.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import { Dialog } from "@uniflowed/ui";

import type { ButtonSize, ButtonTone } from "./button.js";
import { Button } from "./button.js";

/**
 * Every prop a caller passes that this file does not name, on its way to the
 * part underneath. See `forwarded` for the one thing Flow cannot say about it.
 */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  overlay: {
    position: "fixed",
    inset: 0,
    zIndex: 50,
    backgroundColor: ufTokens.scrim,
    // Enter: the page dims as the panel arrives, over the panel's duration,
    // rather than going dark first and then showing a dialog. Opacity only, so
    // it is the same under reduced motion.
    opacity: { default: 1, "@starting-style": 0 },
    transitionProperty: "opacity",
    transitionDuration: ufTokens.durationSlow,
    transitionTimingFunction: ufTokens.easingEnter,
  },
  // Centred by `inset: 0` and `margin: auto` rather than by a translate, which
  // leaves text on a half pixel and blurs it on a screen with no scaling.
  panel: {
    position: "fixed",
    inset: 0,
    zIndex: 50,
    boxSizing: "border-box",
    display: "grid",
    gap: ufTokens.space4,
    width: "calc(100% - 32px)",
    maxWidth: "32rem",
    height: "fit-content",
    maxHeight: "calc(100% - 32px)",
    margin: "auto",
    overflowY: "auto",
    padding: ufTokens.space6,
    backgroundColor: ufTokens.surface,
    color: ufTokens.ink,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingBase,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusLg,
    // The panel takes focus itself when it holds nothing focusable, so it is
    // drawn like anything else that can.
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
    // Enter: it fades in and comes forward from 96% of its size, as if it
    // rose out of the page. 0.96 is enough to be seen at this size and too
    // little to read as growing from nothing, and there is no overshoot back
    // past 1. `durationSlow`, because a surface this large moving as fast as
    // a menu looks thrown. The scale is gone when it settles (`none`), so the
    // text is not left on a half pixel. Under reduced motion it only fades.
    opacity: { default: 1, "@starting-style": 0 },
    transform: { default: "none", "@starting-style": "scale(0.96)" },
    transitionProperty: {
      default: "opacity, transform",
      "@media (prefers-reduced-motion: reduce)": "opacity",
    },
    transitionDuration: ufTokens.durationSlow,
    transitionTimingFunction: ufTokens.easingEnter,
  },
  header: {
    display: "grid",
    gap: ufTokens.space2,
    // Room for the close button, which is drawn over this corner.
    paddingInlineEnd: ufTokens.space8,
  },
  footer: {
    display: "flex",
    flexWrap: "wrap",
    justifyContent: "flex-end",
    gap: ufTokens.space2,
  },
  title: {
    margin: 0,
    fontSize: ufTokens.textLg,
    fontWeight: ufTokens.weightBold,
    lineHeight: ufTokens.leadingTight,
  },
  description: {
    margin: 0,
    color: ufTokens.muted,
  },
  close: {
    position: "absolute",
    top: ufTokens.space4,
    insetInlineEnd: ufTokens.space4,
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: "32px",
    height: "32px",
    padding: 0,
    borderWidth: 0,
    borderRadius: ufTokens.radiusSm,
    backgroundColor: { default: "transparent", ":hover": ufTokens.surfaceHover },
    color: { default: ufTokens.muted, ":hover": ufTokens.ink },
    cursor: "pointer",
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
});

/** The dialog, open or closed. Uncontrolled unless `open` is given. */
component DialogRoot(
  children: React.Node,
  defaultOpen?: boolean = false,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
) {
  return (
    <Dialog.Root defaultOpen={defaultOpen} onOpenChange={onOpenChange} open={open}>
      {children}
    </Dialog.Root>
  );
}

/**
 * The button that opens the dialog, and that focus comes back to when it
 * closes. It is a `Button`, so it takes a `tone` and a `size`.
 */
component DialogTrigger(
  children: React.Node,
  tone?: ButtonTone = "neutral",
  size?: ButtonSize = "md",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Dialog.Trigger
      {...forwarded(rest)}
      render={(trigger) => (
        <Button
          {...forwarded(trigger)}
          className={className}
          size={size}
          tone={tone}
          xstyle={xstyle}
        />
      )}
    >
      {children}
    </Dialog.Trigger>
  );
}

/**
 * The scrim and the panel, with a close button in the corner.
 *
 * Everything it does not name reaches `Dialog.Body`: `aria-label` for a dialog
 * with no visible title, `initialFocus`, `dismissOnOutsidePress`.
 */
component DialogContent(
  children: React.Node,
  closeLabel?: string = "Close",
  hideClose?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <>
      <Dialog.Overlay className={props(styles.overlay).className} />
      <Dialog.Body
        {...forwarded(rest)}
        className={classNames(props(styles.panel, xstyle).className, className)}
      >
        {children}
        {hideClose ? null : (
          <Dialog.Close aria-label={closeLabel} className={props(styles.close).className}>
            <CloseIcon />
          </Dialog.Close>
        )}
      </Dialog.Body>
    </>
  );
}

/** The title and description, stacked. */
component DialogHeader(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Dialog.Header
      {...forwarded(rest)}
      className={classNames(props(styles.header, xstyle).className, className)}
    >
      {children}
    </Dialog.Header>
  );
}

/** The row the actions sit in, at the end of the reading direction. */
component DialogFooter(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Dialog.Footer
      {...forwarded(rest)}
      className={classNames(props(styles.footer, xstyle).className, className)}
    >
      {children}
    </Dialog.Footer>
  );
}

/**
 * The dialog's name. An `<h2>` unless `render` says which heading it is, which
 * is a fact about the page around the dialog rather than about the dialog.
 */
component DialogTitle(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Dialog.Title
      {...forwarded(rest)}
      className={classNames(props(styles.title, xstyle).className, className)}
    >
      {children}
    </Dialog.Title>
  );
}

/** What the dialog is for, read with its name when focus arrives. */
component DialogDescription(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Dialog.Description
      {...forwarded(rest)}
      className={classNames(props(styles.description, xstyle).className, className)}
    >
      {children}
    </Dialog.Description>
  );
}

/** A button that closes the dialog: Cancel, Done, or the action itself. */
component DialogClose(
  children: React.Node,
  tone?: ButtonTone = "neutral",
  size?: ButtonSize = "md",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Dialog.Close
      {...forwarded(rest)}
      render={(close) => (
        <Button
          {...forwarded(close)}
          className={className}
          size={size}
          tone={tone}
          xstyle={xstyle}
        />
      )}
    >
      {children}
    </Dialog.Close>
  );
}

/** The `×`, which is decoration: the button it sits in carries the name. */
component CloseIcon() {
  return (
    <svg
      aria-hidden="true"
      fill="none"
      focusable="false"
      height="16"
      stroke="currentColor"
      strokeLinecap="round"
      strokeWidth="2"
      viewBox="0 0 24 24"
      width="16"
    >
      <path d="M18 6 6 18M6 6l12 12" />
    </svg>
  );
}

/**
 * A caller's props on their way into a part, rather than onto an element.
 *
 * Flow checks a spread into a component against that component's own
 * `...rest`, `key` included, and the indexer in `Rest` answers `mixed` where
 * the part says `empty`. `@uniflowed/ui` meets the same hole between its own
 * parts, and papers over it the same way in one named place,
 * `internal/merge-props.js`'s `forwarded`: nothing that was ever checked is
 * lost, because every element those parts render has `any`-typed props in uf's
 * library today.
 */
function forwarded(rest: Rest): $FlowFixMe {
  return rest;
}

/** The classes this file chose, then the caller's. See `button.js`. */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}

/**
 * The parts, under the names `import * as Dialog from "./dialog.js"` gives them.
 *
 * One name per component (ubugeeei-prod/uf#1453): a page writes `<Dialog.Root>`
 * and `<Dialog.Trigger>`, the way it writes `@uniflowed/ui`'s own parts. Each
 * is declared under its full name, so React DevTools and an error say
 * `DialogRoot` rather than `Root`.
 */
export {
  DialogRoot as Root,
  DialogTrigger as Trigger,
  DialogContent as Content,
  DialogHeader as Header,
  DialogFooter as Footer,
  DialogTitle as Title,
  DialogDescription as Description,
  DialogClose as Close,
};
