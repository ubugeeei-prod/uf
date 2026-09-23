"use client";
// @flow
//
// Tooltip: a short phrase about a control, shown on hover and on focus, and
// never in the way.
//
// `uf ui add tooltip` wrote this file into the project, and it is the
// project's from then on. `uf ui diff tooltip` shows how it has moved away from
// the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The look: a small dark label beside its trigger. The behaviour is imported
// from `@uniflowed/ui`'s `Tooltip`, and it is what WCAG 1.4.13 asks of content that
// appears on hover: it waits for a pointer and not for focus, it stays while
// the pointer travels onto it, `Escape` dismisses it from wherever focus is,
// and it describes its trigger with `aria-describedby` only while it is there.
// `TooltipProvider` shares one clock across a toolbar, so the second icon a
// reader points at answers at once. A fix to any of that reaches this project
// by upgrading `@uniflowed/ui`.
//
// # What to keep true when you change it
//
// * **It holds a phrase and nothing a reader can act on.** A tooltip never
//   takes focus, so a link or a button inside one is one nobody can reach. A
//   preview with links in it is a `hover-card`.
// * **The trigger has its own name.** A tooltip does not open on touch, so an
//   icon button's name is its `aria-label`, and the tooltip only repeats it.
// * **It is inverted, and the pair is measured.** `canvas` on `ink` is the pair
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 as `ink` on `canvas`
//   — contrast is the same in both directions — and it stays inverted in the
//   dark theme, where the two swap.
// * **It is short.** Capped at 20rem; a tooltip that needs more is a
//   description on the page.

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
  content: {
    zIndex: 60,
    boxSizing: "border-box",
    maxWidth: "20rem",
    margin: 0,
    paddingBlock: ufTokens.space1,
    paddingInline: ufTokens.space2,
    backgroundColor: ufTokens.ink,
    color: ufTokens.canvas,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textXs,
    fontWeight: ufTokens.weightMedium,
    lineHeight: ufTokens.leadingTight,
    overflowWrap: "break-word",
    borderRadius: ufTokens.radiusSm,
  },
});

/**
 * The clock a group of tooltips shares, so the next one opens at once once a
 * reader has waited for the first.
 */
export component TooltipProvider(
  children: React.Node,
  delayDuration?: number,
  skipDelayDuration?: number,
) {
  return (
    <Primitive.TooltipProvider delayDuration={delayDuration} skipDelayDuration={skipDelayDuration}>
      {children}
    </Primitive.TooltipProvider>
  );
}

/** One tooltip and its trigger. */
export component Tooltip(
  children: React.Node,
  openDelay?: number,
  closeDelay?: number,
  defaultOpen?: boolean = false,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
) {
  return (
    <Primitive.TooltipRoot
      closeDelay={closeDelay}
      defaultOpen={defaultOpen}
      onOpenChange={onOpenChange}
      open={open}
      openDelay={openDelay}
    >
      {children}
    </Primitive.TooltipRoot>
  );
}

/**
 * The control the tooltip describes. It is a `Button` unless `render` says
 * otherwise, and whatever it is needs a name of its own.
 */
export component TooltipTrigger(
  children?: React.Node,
  tone?: ButtonTone = "neutral",
  size?: ButtonSize = "md",
  render?: RenderProp,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.TooltipTrigger
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
    </Primitive.TooltipTrigger>
  );
}

/**
 * The phrase, above its trigger unless it does not fit. `side` and `align`
 * reach `Tooltip.Body` untouched; `sideOffset` is the gap.
 */
export component TooltipContent(
  children: React.Node,
  sideOffset?: number = 6,
  collisionPadding?: number = 8,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.TooltipBody
      {...forwarded(rest)}
      className={classNames(props(styles.content, xstyle).className, className)}
      collisionPadding={collisionPadding}
      sideOffset={sideOffset}
    >
      {children}
    </Primitive.TooltipBody>
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
