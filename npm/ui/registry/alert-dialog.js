"use client";
// @flow
//
// Alert dialog: a confirmation that interrupts, says what answering costs,
// and waits for an answer.
//
// `uf ui add alert-dialog` wrote this file into the project, and it is the
// project's from then on. `uf ui diff alert-dialog` shows how it has moved away
// from the registry in the uf you are running.
//
// # When this rather than `dialog`
//
// Only when the action is destructive or has to be answered before anything
// else happens: deleting, discarding, signing out with unsaved work. Everything
// else is a `dialog`, because a reader who is interrupted for a question that
// did not need asking learns to press the first button without reading.
//
// # What this file owns, and what it does not
//
// The look: the scrim, the panel, the two answers in the footer. The behaviour
// is imported from `@uniflowed/ui`'s `AlertDialog`: `role="alertdialog"`, which
// makes a screen reader announce the description the moment focus arrives;
// focus starting on the answer that does least; no dismissal by a press beside
// the panel; `Escape` as the way to decline; focus given back to the trigger;
// and the page behind made inert and held still. A fix to any of that reaches
// this project by upgrading `@uniflowed/ui`.
//
// # What to keep true when you change it
//
// * **It always has a description.** `AlertDialog.Description` is the sentence
//   the role exists to announce, and the part raises when there is none — an
//   alert dialog with nothing to say interrupts the reader to say nothing.
// * **Focus starts on `AlertDialog.Cancel`.** Keep one in the footer. The least
//   destructive answer is where a reader who pressed `Enter` too early should
//   land.
// * **The destructive answer looks destructive.** `AlertDialog.Action` takes a
//   `tone`, and a deletion is `tone="danger"`; a confirmation whose two buttons
//   look the same is one where the colour carries no warning.
// * **There is no close button in the corner.** A confirmation is answered,
//   not dismissed, and `Escape` is already the way to decline.
// * **Colour comes from tokens, in measured pairs.** `ink` and `muted` on
//   `surface`, and the buttons' own pairs, which
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in the light default
//   and the dark theme.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import { AlertDialog } from "@uniflowed/ui";

import type { ButtonSize, ButtonTone } from "./button.js";
import { Button } from "./button.js";

/**
 * Every prop a caller passes that this file does not name, on its way to the
 * part underneath. See `forwarded` for the one thing Flow cannot say about it.
 */
type Rest = { readonly key?: empty, readonly [string]: mixed };

/** A caller's own element in place of the one a part renders. */
type RenderProp = (props: Rest) => React.Node;

const styles = stylex.create({
  overlay: {
    position: "fixed",
    inset: 0,
    zIndex: 50,
    backgroundColor: ufTokens.scrim,
    // Enter: the page dims as the panel arrives, over the panel's duration,
    // rather than going dark first and then showing a dialog. Opacity only, so
    // it is the same under reduced motion.
    // Exit: it clears with the panel, in the panel's shorter exit time.
    opacity: { default: 1, "@starting-style": 0, ":is([data-state=closed])": 0 },
    transitionProperty: "opacity",
    transitionDuration: {
      default: ufTokens.durationSlow,
      ":is([data-state=closed])": ufTokens.durationBase,
    },
    transitionTimingFunction: {
      default: ufTokens.easingEnter,
      ":is([data-state=closed])": ufTokens.easingExit,
    },
  },
  panel: {
    position: "fixed",
    inset: 0,
    zIndex: 50,
    boxSizing: "border-box",
    display: "grid",
    gap: ufTokens.space4,
    width: "calc(100% - 32px)",
    maxWidth: "28rem",
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
    //
    // Exit: it sinks back to 96% as it fades, in `durationBase` on the
    // accelerating curve, so it is out of the way sooner than it arrived.
    // `@uniflowed/ui` keeps it on the page, closed and `inert`, until then,
    // and focus is already back on the trigger. Under reduced motion it only
    // fades: `--uf-exit-travel` is 0 there, so the scale does not jump.
    "--uf-exit-travel": { default: "1", "@media (prefers-reduced-motion: reduce)": "0" },
    opacity: { default: 1, "@starting-style": 0, ":is([data-state=closed])": 0 },
    transform: {
      default: "none",
      "@starting-style": "scale(0.96)",
      ":is([data-state=closed])": "scale(calc(1 - 0.04 * var(--uf-exit-travel)))",
    },
    transitionProperty: {
      default: "opacity, transform",
      "@media (prefers-reduced-motion: reduce)": "opacity",
    },
    transitionDuration: {
      default: ufTokens.durationSlow,
      ":is([data-state=closed])": ufTokens.durationBase,
    },
    transitionTimingFunction: {
      default: ufTokens.easingEnter,
      ":is([data-state=closed])": ufTokens.easingExit,
    },
  },
  header: {
    display: "grid",
    gap: ufTokens.space2,
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
});

/** The alert dialog, open or closed. Uncontrolled unless `open` is given. */
component AlertDialogRoot(
  children: React.Node,
  defaultOpen?: boolean = false,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
) {
  return (
    <AlertDialog.Root defaultOpen={defaultOpen} onOpenChange={onOpenChange} open={open}>
      {children}
    </AlertDialog.Root>
  );
}

/**
 * The button that asks the question, and that focus comes back to. It is a
 * `Button` unless `render` says otherwise.
 */
component AlertDialogTrigger(
  children: React.Node,
  tone?: ButtonTone = "neutral",
  size?: ButtonSize = "md",
  render?: RenderProp,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <AlertDialog.Trigger
      {...forwarded(rest)}
      render={
        render ??
        ((trigger) => (
          <Button
            {...forwarded(trigger)}
            className={className}
            size={size}
            tone={tone}
            xstyle={xstyle}
          />
        ))
      }
    >
      {children}
    </AlertDialog.Trigger>
  );
}

/** The scrim and the panel. There is no close button: see the header. */
component AlertDialogContent(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <>
      <AlertDialog.Overlay className={props(styles.overlay).className} />
      <AlertDialog.Body
        {...forwarded(rest)}
        className={classNames(props(styles.panel, xstyle).className, className)}
      >
        {children}
      </AlertDialog.Body>
    </>
  );
}

/** The question and what answering costs, stacked. */
component AlertDialogHeader(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <AlertDialog.Header
      {...forwarded(rest)}
      className={classNames(props(styles.header, xstyle).className, className)}
    >
      {children}
    </AlertDialog.Header>
  );
}

/** The row the two answers sit in, at the end of the reading direction. */
component AlertDialogFooter(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <AlertDialog.Footer
      {...forwarded(rest)}
      className={classNames(props(styles.footer, xstyle).className, className)}
    >
      {children}
    </AlertDialog.Footer>
  );
}

/** The question, which is the alert dialog's accessible name. */
component AlertDialogTitle(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <AlertDialog.Title
      {...forwarded(rest)}
      className={classNames(props(styles.title, xstyle).className, className)}
    >
      {children}
    </AlertDialog.Title>
  );
}

/** What answering costs, announced the moment focus arrives. Required. */
component AlertDialogDescription(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <AlertDialog.Description
      {...forwarded(rest)}
      className={classNames(props(styles.description, xstyle).className, className)}
    >
      {children}
    </AlertDialog.Description>
  );
}

/** The answer that does the thing. A deletion is `tone="danger"`. */
component AlertDialogAction(
  children: React.Node,
  tone?: ButtonTone = "primary",
  size?: ButtonSize = "md",
  render?: RenderProp,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <AlertDialog.Action
      {...forwarded(rest)}
      render={
        render ??
        ((action) => (
          <Button
            {...forwarded(action)}
            className={className}
            size={size}
            tone={tone}
            xstyle={xstyle}
          />
        ))
      }
    >
      {children}
    </AlertDialog.Action>
  );
}

/** The answer that declines, and the one focus starts on. */
component AlertDialogCancel(
  children: React.Node,
  tone?: ButtonTone = "neutral",
  size?: ButtonSize = "md",
  render?: RenderProp,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <AlertDialog.Cancel
      {...forwarded(rest)}
      render={
        render ??
        ((cancel) => (
          <Button
            {...forwarded(cancel)}
            className={className}
            size={size}
            tone={tone}
            xstyle={xstyle}
          />
        ))
      }
    >
      {children}
    </AlertDialog.Cancel>
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
 * The parts, under the names `import * as AlertDialog from "./alert-dialog.js"` gives them.
 *
 * One name per component (ubugeeei-prod/uf#1453): a page writes `<AlertDialog.Root>`
 * and `<AlertDialog.Trigger>`, the way it writes `@uniflowed/ui`'s own parts. Each
 * is declared under its full name, so React DevTools and an error say
 * `AlertDialogRoot` rather than `Root`.
 */
export {
  AlertDialogRoot as Root,
  AlertDialogTrigger as Trigger,
  AlertDialogContent as Content,
  AlertDialogHeader as Header,
  AlertDialogFooter as Footer,
  AlertDialogTitle as Title,
  AlertDialogDescription as Description,
  AlertDialogAction as Action,
  AlertDialogCancel as Cancel,
};
