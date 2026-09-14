"use client";
// @flow
//
// Sheet: a modal panel against one edge of the screen, for the settings or
// filters a page slides in beside itself.
//
// `uf ui add sheet` wrote this file into the project, and it is the project's
// from then on. `uf ui diff sheet` shows how it has moved away from the
// registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The look: the scrim, a panel attached to `side`, the close button in its
// corner. The behaviour is imported from `@uniflowed/ui/sheet`, which is
// `@uniflowed/ui/dialog` with an edge: focus moved in and kept in, `Escape`,
// the press outside, focus given back, the page behind made inert and held
// still. The part writes the edge as `data-side` on the panel, and every
// position below is drawn from that attribute through
// `:is([data-side=left])`, so the panel and the part cannot come to disagree
// about which edge "left" is.
//
// # What to keep true when you change it
//
// * **`side` is physical.** `left` is the left of the screen in every writing
//   direction, because a design that puts a panel against the left edge means
//   that edge; what the writing direction changes is the reading order inside.
// * **A sheet needs a name.** `SheetTitle` is what `aria-labelledby` points
//   at. A design with no visible title passes `aria-label` to `SheetContent`.
// * **The close button is last in the markup and first on the screen.** Focus
//   moves to the first thing worth acting on, which should be the sheet's own
//   content rather than the corner `×`. Its name is `closeLabel`.
// * **It scrolls inside, and it never covers everything.** A side sheet is
//   capped at 24rem and a top or bottom one at 32rem, each less a margin, so
//   part of the page stays visible behind the scrim and a reader can see they
//   are still on it.
// * **Colour comes from tokens, in measured pairs.** `ink` and `muted` on
//   `surface`, which `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in
//   the light default and the dark theme.
// * **There is no slide.** uf's StyleX has no `@keyframes` yet, and a
//   transition needs the panel mounted while closed, which `@uniflowed/ui` does
//   not do.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import type { Edge } from "@uniflowed/ui/sheet";
import * as Primitive from "@uniflowed/ui/sheet";

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
    zIndex: 50,
    boxSizing: "border-box",
    display: "flex",
    flexDirection: "column",
    gap: ufTokens.space4,
    overflowY: "auto",
    padding: ufTokens.space6,
    backgroundColor: ufTokens.surface,
    color: ufTokens.ink,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingBase,
    boxShadow: ufTokens.shadowPanel,
    // Attached to every edge except the one opposite `side`, which is what
    // makes a side sheet as tall as the screen and a top one as wide as it.
    top: { default: 0, ":is([data-side=bottom])": "auto" },
    bottom: { default: 0, ":is([data-side=top])": "auto" },
    left: { default: 0, ":is([data-side=right])": "auto" },
    right: { default: 0, ":is([data-side=left])": "auto" },
    // Literals rather than shared constants: StyleX reads a value at compile
    // time, and a name it would have to resolve is refused.
    width: {
      default: "auto",
      ":is([data-side=left])": "min(24rem, calc(100% - 48px))",
      ":is([data-side=right])": "min(24rem, calc(100% - 48px))",
    },
    maxHeight: {
      default: "none",
      ":is([data-side=top])": "min(32rem, calc(100% - 48px))",
      ":is([data-side=bottom])": "min(32rem, calc(100% - 48px))",
    },
    // A line on the edge that meets the page, and rounded corners on it.
    borderWidth: 0,
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderLeftWidth: { default: 0, ":is([data-side=right])": "1px" },
    borderRightWidth: { default: 0, ":is([data-side=left])": "1px" },
    borderTopWidth: { default: 0, ":is([data-side=bottom])": "1px" },
    borderBottomWidth: { default: 0, ":is([data-side=top])": "1px" },
    borderTopLeftRadius: { default: 0, ":is([data-side=bottom])": ufTokens.radiusXl },
    borderTopRightRadius: { default: 0, ":is([data-side=bottom])": ufTokens.radiusXl },
    borderBottomLeftRadius: { default: 0, ":is([data-side=top])": ufTokens.radiusXl },
    borderBottomRightRadius: { default: 0, ":is([data-side=top])": ufTokens.radiusXl },
    // The panel takes focus itself when it holds nothing focusable.
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "-2px",
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
    marginTop: "auto",
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

/** The sheet, open or closed, against `side`. Uncontrolled unless `open` is given. */
export component Sheet(
  children: React.Node,
  side?: Edge = "right",
  defaultOpen?: boolean = false,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
) {
  return (
    <Primitive.SheetRoot
      defaultOpen={defaultOpen}
      onOpenChange={onOpenChange}
      open={open}
      side={side}
    >
      {children}
    </Primitive.SheetRoot>
  );
}

/** The button that opens the sheet. It is a `Button` unless `render` says otherwise. */
export component SheetTrigger(
  children: React.Node,
  tone?: ButtonTone = "neutral",
  size?: ButtonSize = "md",
  render?: RenderProp,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.SheetTrigger
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
    </Primitive.SheetTrigger>
  );
}

/**
 * The scrim and the panel, with a close button in the corner. Everything it
 * does not name reaches `Sheet.Body`, `aria-label` included.
 */
export component SheetContent(
  children: React.Node,
  closeLabel?: string = "Close",
  hideClose?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <>
      <Primitive.SheetOverlay className={props(styles.overlay).className} />
      <Primitive.SheetBody
        {...forwarded(rest)}
        className={classNames(props(styles.panel, xstyle).className, className)}
      >
        {children}
        {hideClose ? null : (
          <Primitive.SheetClose aria-label={closeLabel} className={props(styles.close).className}>
            <CloseIcon />
          </Primitive.SheetClose>
        )}
      </Primitive.SheetBody>
    </>
  );
}

/** The title and description, stacked. */
export component SheetHeader(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.SheetHeader
      {...forwarded(rest)}
      className={classNames(props(styles.header, xstyle).className, className)}
    >
      {children}
    </Primitive.SheetHeader>
  );
}

/** The row the actions sit in, pushed to the far end of the panel. */
export component SheetFooter(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.SheetFooter
      {...forwarded(rest)}
      className={classNames(props(styles.footer, xstyle).className, className)}
    >
      {children}
    </Primitive.SheetFooter>
  );
}

/** The sheet's name. */
export component SheetTitle(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.SheetTitle
      {...forwarded(rest)}
      className={classNames(props(styles.title, xstyle).className, className)}
    >
      {children}
    </Primitive.SheetTitle>
  );
}

/** What the sheet is for, read with its name when focus arrives. */
export component SheetDescription(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.SheetDescription
      {...forwarded(rest)}
      className={classNames(props(styles.description, xstyle).className, className)}
    >
      {children}
    </Primitive.SheetDescription>
  );
}

/** A button that closes the sheet: Done, Apply, or Cancel. */
export component SheetClose(
  children: React.Node,
  tone?: ButtonTone = "neutral",
  size?: ButtonSize = "md",
  render?: RenderProp,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.SheetClose
      {...forwarded(rest)}
      render={
        render ??
        ((close) => (
          <Button
            {...forwarded(close)}
            className={className}
            size={size}
            tone={tone}
            xstyle={xstyle}
          />
        ))
      }
    >
      {children}
    </Primitive.SheetClose>
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
