"use client";
// @flow
//
// Resizable: two panels side by side or stacked, with a handle between them
// that moves the boundary.
//
// `uf ui add resizable` wrote this file into the project, and it is the
// project's from then on. `uf ui diff resizable` shows how it has moved away
// from the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The group's direction, the panels' overflow, and the handle's line, the area
// around it a pointer can land on, and its ring. How big each panel is comes
// from what `@uniflowed/ui`'s `Resizable` writes: `--uf-resizable-size` on each
// panel, a percentage of the group. The part owns what the handle is: a
// focusable `role="separator"` with `aria-valuenow` between `aria-valuemin` and
// `aria-valuemax`, `aria-controls` naming the primary panel, the keys that move
// it, and a drag.
//
// # What to keep true when you change it
//
// * **Name the handle.** `label` says what it resizes, such as "Resize
//   sidebar", when a page has more than one.
// * **Keep the ring.** The handle is a tab stop, and its outline is how a
//   sighted keyboard user finds it.
// * **Mark one panel `primary`.** It is the panel the value measures and the
//   handle's `aria-controls` names; the other takes what is left.
// * **Keep the hit area.** The line is one pixel wide; the transparent area
//   either side of it is what a pointer can actually grab.

import * as React from "@uniflowed/react";
import { createContext, useContext } from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import * as Primitive from "@uniflowed/ui";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

type Orientation = "horizontal" | "vertical";

/** Which way the group runs, for the handle, which draws its line across it. */
const OrientationContext = createContext<Orientation>("horizontal");

const styles = stylex.create({
  group: {
    display: "flex",
    boxSizing: "border-box",
    width: "100%",
    height: "100%",
    overflow: "hidden",
  },
  groupVertical: {
    flexDirection: "column",
  },
  panel: {
    flexBasis: "var(--uf-resizable-size, 50%)",
    flexGrow: 1,
    flexShrink: 1,
    minWidth: 0,
    minHeight: 0,
    overflow: "auto",
  },
  // Nine pixels a pointer can grab, drawn as a one-pixel line: transparent
  // borders either side of a one-pixel box whose background stops at them, and
  // negative margins so the grab area overlaps the panels rather than pushing
  // them apart.
  handle: {
    position: "relative",
    zIndex: 1,
    flexShrink: 0,
    alignSelf: "stretch",
    boxSizing: "content-box",
    width: "1px",
    marginInlineStart: "-4px",
    marginInlineEnd: "-4px",
    borderInlineStartWidth: "4px",
    borderInlineEndWidth: "4px",
    borderInlineStartStyle: "solid",
    borderInlineEndStyle: "solid",
    borderInlineStartColor: "transparent",
    borderInlineEndColor: "transparent",
    backgroundClip: "padding-box",
    backgroundColor: {
      default: ufTokens.border,
      ":hover": ufTokens.accent,
      ":focus-visible": ufTokens.accent,
      ":is([aria-disabled=true])": ufTokens.border,
    },
    cursor: { default: "col-resize", ":is([aria-disabled=true])": "default" },
    touchAction: "none",
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "0",
  },
  handleVertical: {
    width: "auto",
    height: "1px",
    marginInlineStart: 0,
    marginInlineEnd: 0,
    marginBlockStart: "-4px",
    marginBlockEnd: "-4px",
    borderInlineStartWidth: 0,
    borderInlineEndWidth: 0,
    borderBlockStartWidth: "4px",
    borderBlockEndWidth: "4px",
    borderBlockStartStyle: "solid",
    borderBlockEndStyle: "solid",
    borderBlockStartColor: "transparent",
    borderBlockEndColor: "transparent",
    cursor: { default: "row-resize", ":is([aria-disabled=true])": "default" },
  },
});

/**
 * The group. `value` is the primary panel's share, a percentage between `min`
 * and `max`; `orientation` says whether the panels sit side by side or stacked.
 */
export component ResizablePanelGroup(
  children: React.Node,
  value?: number,
  defaultValue?: number = 50,
  onValueChange?: (value: number) => void,
  min?: number = 0,
  max?: number = 100,
  step?: number = 10,
  orientation?: Orientation = "horizontal",
  disabled?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const vertical = orientation === "vertical";
  return (
    <OrientationContext.Provider value={orientation}>
      <Primitive.ResizablePanelGroup
        {...forwarded(rest)}
        className={classNames(
          props(styles.group, vertical && styles.groupVertical, xstyle).className,
          className,
        )}
        defaultValue={defaultValue}
        disabled={disabled}
        max={max}
        min={min}
        onValueChange={onValueChange}
        orientation={orientation}
        step={step}
        value={value}
      >
        {children}
      </Primitive.ResizablePanelGroup>
    </OrientationContext.Provider>
  );
}

/** One panel. The `primary` one is the panel the group's value measures. */
export component ResizablePanel(
  children: React.Node,
  primary?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.ResizablePanel
      {...forwarded(rest)}
      className={classNames(props(styles.panel, xstyle).className, className)}
      primary={primary}
    >
      {children}
    </Primitive.ResizablePanel>
  );
}

/** The handle between the panels. */
export component ResizableHandle(
  label?: string = "Resize",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const vertical = useContext(OrientationContext) === "vertical";
  return (
    <Primitive.ResizableHandle
      {...forwarded(rest)}
      className={classNames(
        props(styles.handle, vertical && styles.handleVertical, xstyle).className,
        className,
      )}
      label={label}
    />
  );
}

/**
 * A caller's props on their way into a part rather than onto an element. Flow
 * checks that spread against the part's own `...rest`, whose `key` is `empty`
 * where this file's indexer says `mixed`; `@uniflowed/ui` papers over the same
 * hole the same way, and nothing checked is lost, because the elements its
 * parts render have `any`-typed props in uf's library today.
 */
function forwarded(rest: Rest): $FlowFixMe {
  return rest;
}

/** The classes this file chose, then the caller's. */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
