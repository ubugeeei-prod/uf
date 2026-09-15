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
// * **It always has a description.** `AlertDialogDescription` is the sentence
//   the role exists to announce, and the part raises when there is none — an
//   alert dialog with nothing to say interrupts the reader to say nothing.
// * **Focus starts on `AlertDialogCancel`.** Keep one in the footer. The least
//   destructive answer is where a reader who pressed `Enter` too early should
//   land.
// * **The destructive answer looks destructive.** `AlertDialogAction` takes a
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
import * as Primitive from "@uniflowed/ui";

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
    borderRadius: ufTokens.radiusXl,
    boxShadow: ufTokens.shadowPanel,
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
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
export component AlertDialog(
  children: React.Node,
  defaultOpen?: boolean = false,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
) {
  return (
    <Primitive.AlertDialogRoot defaultOpen={defaultOpen} onOpenChange={onOpenChange} open={open}>
      {children}
    </Primitive.AlertDialogRoot>
  );
}

/**
 * The button that asks the question, and that focus comes back to. It is a
 * `Button` unless `render` says otherwise.
 */
export component AlertDialogTrigger(
  children: React.Node,
  tone?: ButtonTone = "neutral",
  size?: ButtonSize = "md",
  render?: RenderProp,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.AlertDialogTrigger
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
    </Primitive.AlertDialogTrigger>
  );
}

/** The scrim and the panel. There is no close button: see the header. */
export component AlertDialogContent(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <>
      <Primitive.AlertDialogOverlay className={props(styles.overlay).className} />
      <Primitive.AlertDialogBody
        {...forwarded(rest)}
        className={classNames(props(styles.panel, xstyle).className, className)}
      >
        {children}
      </Primitive.AlertDialogBody>
    </>
  );
}

/** The question and what answering costs, stacked. */
export component AlertDialogHeader(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.AlertDialogHeader
      {...forwarded(rest)}
      className={classNames(props(styles.header, xstyle).className, className)}
    >
      {children}
    </Primitive.AlertDialogHeader>
  );
}

/** The row the two answers sit in, at the end of the reading direction. */
export component AlertDialogFooter(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.AlertDialogFooter
      {...forwarded(rest)}
      className={classNames(props(styles.footer, xstyle).className, className)}
    >
      {children}
    </Primitive.AlertDialogFooter>
  );
}

/** The question, which is the alert dialog's accessible name. */
export component AlertDialogTitle(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.AlertDialogTitle
      {...forwarded(rest)}
      className={classNames(props(styles.title, xstyle).className, className)}
    >
      {children}
    </Primitive.AlertDialogTitle>
  );
}

/** What answering costs, announced the moment focus arrives. Required. */
export component AlertDialogDescription(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.AlertDialogDescription
      {...forwarded(rest)}
      className={classNames(props(styles.description, xstyle).className, className)}
    >
      {children}
    </Primitive.AlertDialogDescription>
  );
}

/** The answer that does the thing. A deletion is `tone="danger"`. */
export component AlertDialogAction(
  children: React.Node,
  tone?: ButtonTone = "primary",
  size?: ButtonSize = "md",
  render?: RenderProp,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.AlertDialogAction
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
    </Primitive.AlertDialogAction>
  );
}

/** The answer that declines, and the one focus starts on. */
export component AlertDialogCancel(
  children: React.Node,
  tone?: ButtonTone = "neutral",
  size?: ButtonSize = "md",
  render?: RenderProp,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.AlertDialogCancel
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
    </Primitive.AlertDialogCancel>
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
