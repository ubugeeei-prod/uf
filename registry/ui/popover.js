"use client";
// @flow
//
// Popover: a small panel anchored to the button that opened it, with the page
// still usable around it.
//
// `uf ui add popover` wrote this file into the project, and it is the
// project's from then on. `uf ui diff popover` shows how it has moved away from
// the registry in the uf you are running.
//
// # When this rather than `dialog`
//
// When the page behind should stay usable: a filter, a share link, a colour
// picker. A popover is `role="dialog"` with no `aria-modal`, nothing inert and
// no focus trap, so `Tab` leaves it and closes it. A task a reader must finish
// before doing anything else is a `dialog`.
//
// # What this file owns, and what it does not
//
// The look: the panel, its width, and its height against the room the page has
// left. The behaviour is imported from `@uniflowed/ui`'s `Popover`: focus moved in
// when it opens, `Escape` and a press outside that close it and give focus back
// to the trigger, `Tab` that leaves, and the panel kept against its trigger as
// the page scrolls, flipped to the other side when it does not fit. The part
// writes the room it found as `--uf-anchor-available-height`, and the panel
// below is never taller than that.
//
// # What to keep true when you change it
//
// * **It is named by its trigger.** A popover with no `aria-label` of its own
//   is announced with the trigger's text, so the trigger's text should say
//   what the popover is for.
// * **Focus goes in and comes back.** Keep something focusable inside, or the
//   panel takes focus itself — and then it needs the focus ring it has here.
// * **It never hides the only way to do something.** A popover can be closed
//   by moving focus away from it, which is right for a shortcut and wrong for
//   the one place an action lives.
// * **Colour comes from tokens, in measured pairs.** `ink` on `surface`, which
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
  content: {
    zIndex: 50,
    boxSizing: "border-box",
    width: "18rem",
    maxWidth: "calc(100vw - 16px)",
    maxHeight: "var(--uf-anchor-available-height, none)",
    overflowY: "auto",
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
    boxShadow: ufTokens.shadowPanel,
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
});

/** The popover, open or closed. Uncontrolled unless `open` is given. */
export component Popover(
  children: React.Node,
  defaultOpen?: boolean = false,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
) {
  return (
    <Primitive.PopoverRoot defaultOpen={defaultOpen} onOpenChange={onOpenChange} open={open}>
      {children}
    </Primitive.PopoverRoot>
  );
}

/**
 * The button that opens the popover, names it, and gets focus back. It is a
 * `Button` unless `render` says otherwise.
 */
export component PopoverTrigger(
  children: React.Node,
  tone?: ButtonTone = "neutral",
  size?: ButtonSize = "md",
  render?: RenderProp,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.PopoverTrigger
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
    </Primitive.PopoverTrigger>
  );
}

/**
 * The panel, under its trigger unless it does not fit. `side`, `align` and
 * `initialFocus` reach `Popover.Body` untouched; `sideOffset` is the gap.
 */
export component PopoverContent(
  children: React.Node,
  sideOffset?: number = 8,
  collisionPadding?: number = 8,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.PopoverBody
      {...forwarded(rest)}
      className={classNames(props(styles.content, xstyle).className, className)}
      collisionPadding={collisionPadding}
      sideOffset={sideOffset}
    >
      {children}
    </Primitive.PopoverBody>
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
