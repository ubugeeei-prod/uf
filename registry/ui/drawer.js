"use client";
// @flow
//
// Drawer: a sheet with a handle, dragged between snap points or moved there
// with the arrow keys.
//
// `uf ui add drawer` wrote this file into the project, and it is the project's
// from then on. `uf ui diff drawer` shows how it has moved away from the
// registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The look: the panel at its snap point, the grip, and the way the panel
// follows a drag. The behaviour is imported from `@uniflowed/ui`'s `Drawer`: every
// modal promise a sheet makes, a handle that is a `role="slider"` over the snap
// points with `Home`, `End` and the arrow keys, the closing key at the smallest
// snap point, and the refusal to render a handle with no `DrawerClose` beside
// it. A fix to any of that reaches this project by upgrading `@uniflowed/ui`.
//
// The part writes two numbers straight onto the panel rather than rendering
// them: `--uf-drawer-snap`, the fraction of the full size it is open to, and
// `--uf-drawer-drag`, how far a finger has pulled it towards its edge. The
// panel's size and position below are those two properties and nothing else.
//
// # What to keep true when you change it
//
// * **A drag is never the only way.** WCAG 2.2 SC 2.5.7 asks for everything a
//   drag does to be possible without one, so a drawer with a handle needs a
//   `DrawerClose`, and the part raises without one. The handle's arrow keys are
//   the keyboard's half of the same rule.
// * **The handle is a control, so it looks like one.** It is in the tab order,
//   it has a focus ring, and its target is at least 24px across. Its name is
//   `handleLabel`, because a slider with no name is announced as "slider".
// * **The handle comes last in the markup.** Focus moves to the first thing
//   worth acting on when the drawer opens, and that is its content; the handle
//   is drawn at the edge by position rather than by order.
// * **A drag follows the finger, and only a drag does.** The panel eases between
//   snap points, stops easing while it is being dragged, and never eases for a
//   reader who asked for reduced motion.
// * **Colour comes from tokens, in measured pairs.** `ink` and `muted` on
//   `surface`, which `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in
//   the light default and the dark theme.

import * as React from "@uniflowed/react";
import { createContext, useContext } from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import type { Edge } from "@uniflowed/ui";
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

/**
 * The edge the caller gave `Drawer`, for the handle, which is drawn along it.
 * The caller's own prop handed down rather than state the part owns, so there
 * is nothing here that could disagree with the part.
 */
const SideContext: React.Context<Edge> = createContext("bottom");

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
    paddingBlock: ufTokens.space4,
    paddingInline: ufTokens.space6,
    backgroundColor: ufTokens.surface,
    color: ufTokens.ink,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingBase,
    // Separated from the page by a rule on the edge that faces it, not by a
    // shadow: the scrim already says the page is behind.
    borderWidth: 0,
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderTopWidth: { default: 0, ":is([data-side=bottom])": "1px" },
    borderBottomWidth: { default: 0, ":is([data-side=top])": "1px" },
    borderLeftWidth: { default: 0, ":is([data-side=right])": "1px" },
    borderRightWidth: { default: 0, ":is([data-side=left])": "1px" },
    top: { default: 0, ":is([data-side=bottom])": "auto" },
    bottom: { default: 0, ":is([data-side=top])": "auto" },
    left: { default: 0, ":is([data-side=right])": "auto" },
    right: { default: 0, ":is([data-side=left])": "auto" },
    // The snap point is a fraction of the full size, written by the part. The
    // full size is at most 85% of the screen's height, or 24rem across less a
    // margin, spelled out in each value because StyleX reads values at compile
    // time and refuses one it would have to resolve.
    height: {
      default: "auto",
      ":is([data-side=top])": "calc(min(85vh, 40rem) * var(--uf-drawer-snap, 1))",
      ":is([data-side=bottom])": "calc(min(85vh, 40rem) * var(--uf-drawer-snap, 1))",
    },
    width: {
      default: "auto",
      ":is([data-side=left])": "calc(min(24rem, 100vw - 48px) * var(--uf-drawer-snap, 1))",
      ":is([data-side=right])": "calc(min(24rem, 100vw - 48px) * var(--uf-drawer-snap, 1))",
    },
    // And the drag is how far the finger has pulled it towards its edge.
    transform: {
      default: "none",
      ":is([data-side=bottom])": "translateY(var(--uf-drawer-drag, 0px))",
      ":is([data-side=top])": "translateY(calc(-1 * var(--uf-drawer-drag, 0px)))",
      ":is([data-side=right])": "translateX(var(--uf-drawer-drag, 0px))",
      ":is([data-side=left])": "translateX(calc(-1 * var(--uf-drawer-drag, 0px)))",
    },
    transitionProperty: "height, width, transform",
    transitionDuration: {
      default: ufTokens.durationBase,
      ":has([data-dragging=true])": "0s",
      "@media (prefers-reduced-motion: reduce)": "0s",
    },
    transitionTimingFunction: ufTokens.easing,
    borderTopLeftRadius: { default: 0, ":is([data-side=bottom])": ufTokens.radiusLg },
    borderTopRightRadius: { default: 0, ":is([data-side=bottom])": ufTokens.radiusLg },
    borderBottomLeftRadius: { default: 0, ":is([data-side=top])": ufTokens.radiusLg },
    borderBottomRightRadius: { default: 0, ":is([data-side=top])": ufTokens.radiusLg },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "-2px",
  },
  handle: {
    // Read by the grip inside, which is drawn along the axis the drag runs on.
    "--uf-drawer-grip-width": { default: "40px", ":is([aria-orientation=horizontal])": "5px" },
    "--uf-drawer-grip-height": { default: "5px", ":is([aria-orientation=horizontal])": "40px" },
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    flexShrink: 0,
    alignSelf: "center",
    boxSizing: "border-box",
    width: { default: "64px", ":is([aria-orientation=horizontal])": "24px" },
    height: { default: "24px", ":is([aria-orientation=horizontal])": "64px" },
    // First on the screen for a bottom drawer and last for a top one: the edge
    // that faces the page. A side drawer's handle is at that edge's middle.
    order: { default: -1, ":is([data-side=top])": 0 },
    position: { default: "static", ":is([aria-orientation=horizontal])": "absolute" },
    top: { default: "auto", ":is([aria-orientation=horizontal])": "calc(50% - 32px)" },
    left: { default: "auto", ":is([data-side=right])": ufTokens.space1 },
    right: { default: "auto", ":is([data-side=left])": ufTokens.space1 },
    borderRadius: ufTokens.radiusSm,
    cursor: { default: "grab", ":is([data-dragging=true])": "grabbing" },
    touchAction: "none",
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
  grip: {
    width: "var(--uf-drawer-grip-width)",
    height: "var(--uf-drawer-grip-height)",
    borderRadius: ufTokens.radiusPill,
    backgroundColor: ufTokens.border,
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
});

/**
 * The drawer, open or closed, against `side`, at one of `snapPoints`.
 *
 * `snapPoints` are fractions of the full size in ascending order: `[0.5, 1]` is
 * "half, then all of it". `snapPoint` is an index into them.
 */
export component Drawer(
  children: React.Node,
  side?: Edge = "bottom",
  snapPoints?: $ReadOnlyArray<number>,
  defaultSnapPoint?: number = 0,
  snapPoint?: number,
  onSnapPointChange?: (index: number) => void,
  defaultOpen?: boolean = false,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
) {
  return (
    <SideContext.Provider value={side}>
      <Primitive.DrawerRoot
        defaultOpen={defaultOpen}
        defaultSnapPoint={defaultSnapPoint}
        onOpenChange={onOpenChange}
        onSnapPointChange={onSnapPointChange}
        open={open}
        side={side}
        snapPoint={snapPoint}
        snapPoints={snapPoints}
      >
        {children}
      </Primitive.DrawerRoot>
    </SideContext.Provider>
  );
}

/** The button that opens the drawer. It is a `Button` unless `render` says otherwise. */
export component DrawerTrigger(
  children: React.Node,
  tone?: ButtonTone = "neutral",
  size?: ButtonSize = "md",
  render?: RenderProp,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.DrawerTrigger
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
    </Primitive.DrawerTrigger>
  );
}

/**
 * The scrim and the panel, with the handle at the edge that faces the page.
 * `hideHandle` for a drawer that does not move, which then needs no
 * `DrawerClose` either.
 */
export component DrawerContent(
  children: React.Node,
  handleLabel?: string = "Resize the drawer",
  hideHandle?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const side = useContext(SideContext);
  return (
    <>
      <Primitive.DrawerOverlay className={props(styles.overlay).className} />
      <Primitive.DrawerBody
        {...forwarded(rest)}
        className={classNames(props(styles.panel, xstyle).className, className)}
      >
        {children}
        {hideHandle ? null : (
          <Primitive.DrawerHandle
            className={props(styles.handle).className}
            data-side={side}
            label={handleLabel}
            render={(handle) => (
              <div {...forwarded(handle)}>
                <span {...props(styles.grip)} aria-hidden="true" />
              </div>
            )}
          />
        )}
      </Primitive.DrawerBody>
    </>
  );
}

/** The title and description, stacked. */
export component DrawerHeader(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.DrawerHeader
      {...forwarded(rest)}
      className={classNames(props(styles.header, xstyle).className, className)}
    >
      {children}
    </Primitive.DrawerHeader>
  );
}

/** The row the actions sit in, pushed to the far end of the panel. */
export component DrawerFooter(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.DrawerFooter
      {...forwarded(rest)}
      className={classNames(props(styles.footer, xstyle).className, className)}
    >
      {children}
    </Primitive.DrawerFooter>
  );
}

/** The drawer's name. */
export component DrawerTitle(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.DrawerTitle
      {...forwarded(rest)}
      className={classNames(props(styles.title, xstyle).className, className)}
    >
      {children}
    </Primitive.DrawerTitle>
  );
}

/** What the drawer is for, read with its name when focus arrives. */
export component DrawerDescription(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.DrawerDescription
      {...forwarded(rest)}
      className={classNames(props(styles.description, xstyle).className, className)}
    >
      {children}
    </Primitive.DrawerDescription>
  );
}

/**
 * A button that closes the drawer, and the single-pointer alternative to the
 * drag that WCAG 2.5.7 asks for.
 */
export component DrawerClose(
  children: React.Node,
  tone?: ButtonTone = "neutral",
  size?: ButtonSize = "md",
  render?: RenderProp,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.DrawerClose
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
    </Primitive.DrawerClose>
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
