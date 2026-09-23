"use client";
// @flow
//
// Hover card: the preview a link expands into on hover and on focus, with
// links of its own a keyboard can reach.
//
// `uf ui add hover-card` wrote this file into the project, and it is the
// project's from then on. `uf ui diff hover-card` shows how it has moved away
// from the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The look: the trigger as a link, and the card as a panel beside it. The
// behaviour is imported from `@uniflowed/ui`'s `HoverCard`: it waits for a pointer
// and not for focus, it stays while the pointer travels onto it and while focus
// is inside it, `Escape` dismisses it and gives focus back, and it is neither a
// tooltip nor a dialog — its content follows its trigger in the reading order,
// so `Tab` walks from the link into the card. A fix to any of that reaches this
// project by upgrading `@uniflowed/ui`.
//
// # What to keep true when you change it
//
// * **The link works on its own.** A hover card never opens on touch, so
//   `HoverCardTrigger`'s `href` has to go somewhere useful without it, and
//   nothing may live only in the card.
// * **The trigger is focusable.** It is an `<a href>`, and the part refuses a
//   trigger a keyboard cannot reach: a hover card on a `<span>` is one a
//   keyboard reader never sees.
// * **A link looks like a link.** Underlined, because colour alone does not say
//   "link" (WCAG 1.4.1), and in `ink`, whose pairs are measured; the accent is
//   only the underline.
// * **Colour comes from tokens, in measured pairs.** `ink` and `muted` on
//   `surface` and `canvas`, which `crates/uf_stylex/src/tests/preset.rs` holds
//   to 4.5:1 in the light default and the dark theme.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import * as Primitive from "@uniflowed/ui";

/**
 * Every prop a caller passes that this file does not name, on its way to the
 * part underneath. See `forwarded` for the one thing Flow cannot say about it.
 */
type Rest = { readonly key?: empty, readonly [string]: mixed };

/** A caller's own element in place of the one a part renders. */
type RenderProp = (props: Rest) => React.Node;

const styles = stylex.create({
  trigger: {
    color: ufTokens.ink,
    fontWeight: ufTokens.weightMedium,
    textDecorationLine: "underline",
    textDecorationColor: { default: ufTokens.accent, ":hover": ufTokens.ink },
    textDecorationThickness: "2px",
    textUnderlineOffset: "3px",
    borderRadius: ufTokens.radiusSm,
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
  content: {
    zIndex: 50,
    boxSizing: "border-box",
    width: "20rem",
    maxWidth: "calc(100vw - 16px)",
    margin: 0,
    padding: ufTokens.space4,
    backgroundColor: ufTokens.surface,
    color: ufTokens.ink,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingBase,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusMd,
  },
});

/** A hover card and the link it previews. */
export component HoverCard(
  children: React.Node,
  openDelay?: number,
  closeDelay?: number,
  defaultOpen?: boolean = false,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
) {
  return (
    <Primitive.HoverCard.Root
      closeDelay={closeDelay}
      defaultOpen={defaultOpen}
      onOpenChange={onOpenChange}
      open={open}
      openDelay={openDelay}
    >
      {children}
    </Primitive.HoverCard.Root>
  );
}

/**
 * The link the card is about: an `<a href>` unless `render` says otherwise, and
 * whatever it is has to be reachable by keyboard.
 */
export component HoverCardTrigger(
  children: React.Node,
  href?: string,
  render?: RenderProp,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.HoverCard.Trigger
      {...forwarded(rest)}
      render={
        render ??
        ((trigger) => (
          <a
            {...forwarded(trigger)}
            className={classNames(props(styles.trigger, xstyle).className, className)}
            href={href}
          />
        ))
      }
    >
      {children}
    </Primitive.HoverCard.Trigger>
  );
}

/**
 * The card, under its link unless it does not fit. `side` and `align` reach
 * `HoverCard.Body` untouched; `sideOffset` is the gap.
 */
export component HoverCardContent(
  children: React.Node,
  sideOffset?: number = 8,
  collisionPadding?: number = 8,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.HoverCard.Body
      {...forwarded(rest)}
      className={classNames(props(styles.content, xstyle).className, className)}
      collisionPadding={collisionPadding}
      sideOffset={sideOffset}
    >
      {children}
    </Primitive.HoverCard.Body>
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
