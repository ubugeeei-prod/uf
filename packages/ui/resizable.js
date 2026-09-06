// @flow
//
// Two panes and the handle between them — a slider wearing a different role.
//
// The APG calls this a **window splitter**, and it defines it as a separator
// that behaves like a slider: `aria-valuenow`, `aria-valuemin` and
// `aria-valuemax` say how much of the space the primary pane has, and the
// arrow keys change it. That is why it is next to `slider.js` and shares
// `internal/range.js` with it rather than living with the layout components.
//
// Almost every resizable panel on the web is pointer-only, which is a WCAG
// 2.1.1 failure — no keyboard operation at all — and a 2.5.7 one on top of it.
// If uf ships one, the keyboard is the feature, so the keyboard is what this
// module is: there is no drag here yet, and there is a complete key map.
//
// # The two separators in this package are not the same thing
//
// A reader who greps for `separator` finds this and `menu.js`, and they are
// unrelated:
//
//   * `Menu.Separator` is a rule between groups of items. It is not focusable,
//     it has no value, and it exists so a reader moving through a menu is told
//     the group changed.
//   * `Resizable.Handle` is a *window splitter*. It is focusable, it carries a
//     value, and operating it changes the layout.
//
// ARIA gives both the same role because both are separators; only the second
// one is a control. The difference is `tabindex` and `aria-valuenow`, which is
// also how a screen reader tells them apart.
//
// # `aria-orientation` is the separator's, not the layout's
//
// Two panes side by side are divided by a **vertical** line, so a
// `PanelGroup` whose `orientation` is `"horizontal"` renders a handle whose
// `aria-orientation` is `"vertical"`. That inversion is easy to get backwards
// and worth stating: `aria-orientation` on a separator describes the separator,
// and ARIA's default for the role is `horizontal` — so a vertical splitter
// that says nothing is announced as a horizontal rule.
//
// The APG's own pattern page does not mention `aria-orientation` at all, which
// is why implementations differ. This follows the role's definition rather
// than the pattern's silence.
//
// # Two panes, which is the pattern and not a limitation
//
// The window splitter is defined between two panes: a primary one whose size
// is the value, and the rest. A group of five panels is a layout-constraint
// problem — every handle's range depends on every other panel's minimum — and
// it is a different component with a different core, not a bigger version of
// this one. What is here is the pattern, complete.
//
// # Drawing it
//
// Each pane carries its share as `--uf-resizable-size`, a percentage, and the
// caller's stylesheet decides whether that is a width, a height, a `flex-basis`
// or nothing at all. There is no drag: the pointer half is tracked work, and a
// keyboard-only splitter is a working splitter, where a pointer-only one is
// not.

"use client";

import * as React from "@uniflowed/react";
import {
  createContext,
  useContext,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "@uniflowed/react";

import type { Rest } from "./internal/merge-props.js";
import { composeHandlers, withoutComposed } from "./internal/merge-props.js";
import type { Orientation } from "./internal/roving-focus.js";
import { clamp, isReversed } from "./internal/range.js";
import { useControlled } from "./internal/controlled-state.js";

type ResizableState = {|
  readonly base: string,
  /** The primary pane's share of the group, as a percentage. */
  readonly value: number,
  readonly setValue: (value: number) => void,
  readonly min: number,
  readonly max: number,
  readonly step: number,
  /** How the panes are laid out; the handle's own orientation is the other one. */
  readonly orientation: Orientation,
  readonly disabled: boolean,
  readonly hasPrimary: boolean,
  readonly registerPrimary: (present: boolean) => void,
|};

const ResizableContext: React.Context<ResizableState | null> = createContext(null);

hook useResizable(part: string): ResizableState {
  const state = useContext(ResizableContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Resizable.PanelGroup`);
  }
  return state;
}

/**
 * The two panes and their handle.
 *
 * `value` is the primary pane's percentage of the group, which is what the
 * handle announces — the APG's "a decimal value representing the current
 * position of the separator", where 0 is collapsed and 100 is as large as it
 * is allowed to be.
 *
 * `min` is the size the primary pane collapses to. It is 0 by default, so
 * `Enter` collapses the pane entirely; a group whose primary pane should never
 * disappear gives it a floor.
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
  ...rest: Rest
) {
  const base = useId();
  const [share, setShare] = useControlled(value, defaultValue, onValueChange);
  const [hasPrimary, setHasPrimary] = useState(false);

  const state = useMemo(
    () => ({
      base,
      value: clamp(share, min, max),
      setValue: setShare,
      min,
      max,
      step,
      orientation,
      disabled,
      hasPrimary,
      registerPrimary: setHasPrimary,
    }),
    [base, share, setShare, min, max, step, orientation, disabled, hasPrimary],
  );

  return (
    <ResizableContext.Provider value={state}>
      <div {...rest}>{children}</div>
    </ResizableContext.Provider>
  );
}

/**
 * One pane.
 *
 * `primary` marks the one whose size is the value, and the one the handle
 * names with `aria-controls`. Exactly one pane in a group is primary; the
 * other takes what is left. It is a prop rather than "the first one", because
 * the first one in the document is not the first one to mount the moment a
 * caller renders a pane conditionally, and a handle pointing at the wrong pane
 * is a handle that announces someone else's size.
 */
export component ResizablePanel(children: React.Node, primary?: boolean = false, ...rest: Rest) {
  const group = useResizable("Resizable.Panel");
  const passed = withoutComposed(rest, ["style"]);
  const register = group.registerPrimary;

  useEffect(() => {
    if (!primary) {
      return;
    }
    register(true);
    return () => register(false);
  }, [primary, register]);

  return (
    <div
      {...passed}
      id={primary ? `${group.base}-primary` : undefined}
      style={{
        ...(rest.style as $FlowFixMe),
        "--uf-resizable-size": `${String(primary ? group.value : 100 - group.value)}%`,
      }}
    >
      {children}
    </div>
  );
}

/**
 * The splitter: a separator that behaves like a slider.
 *
 * `label` is its accessible name and has a default, because a splitter is a
 * bare bar with no text in it every time — and a focusable separator with no
 * name is announced as "separator", which tells a reader there is a control
 * here and nothing about what it does.
 */
export component ResizableHandle(label?: string = "Resize", ...rest: Rest) {
  const group = useResizable("Resizable.Handle");
  const passed = withoutComposed(rest, ["onKeyDown"]);
  // Where the pane was before `Enter` collapsed it. A ref because nothing
  // renders it: it is a fact about the last keystroke, not about the layout.
  const restoreTo = useRef<number | null>(null);

  const moveBy = (amount: number) => {
    restoreTo.current = null;
    group.setValue(clamp(group.value + amount, group.min, group.max));
  };

  return (
    <div
      {...passed}
      aria-label={label}
      // Only while a primary pane is in the document, for the reason every
      // part of this package repeats: an `aria-controls` naming an id nothing
      // has is worse than saying nothing at all.
      aria-controls={group.hasPrimary ? `${group.base}-primary` : undefined}
      aria-disabled={group.disabled ? "true" : undefined}
      // The separator's own orientation, which is the other axis from the one
      // the panes are laid out along. See the module header.
      aria-orientation={group.orientation === "horizontal" ? "vertical" : "horizontal"}
      aria-valuemax={group.max}
      aria-valuemin={group.min}
      aria-valuenow={group.value}
      onKeyDown={composeHandlers(rest.onKeyDown, (event: $FlowFixMe) => {
        if (group.disabled) {
          return;
        }
        if (event.key === "Enter") {
          event.preventDefault();
          // Collapse, or put it back where it was. The APG gives `Enter` both
          // jobs, and a collapse with no way back is a pane a keyboard reader
          // has thrown away.
          const previous = restoreTo.current;
          if (previous != null) {
            restoreTo.current = null;
            group.setValue(previous);
            return;
          }
          if (group.value === group.min) {
            return;
          }
          restoreTo.current = group.value;
          group.setValue(group.min);
          return;
        }

        if (event.key === "Home" || event.key === "End") {
          event.preventDefault();
          restoreTo.current = null;
          group.setValue(event.key === "Home" ? group.min : group.max);
          return;
        }

        const amount = stepFor(
          event.key,
          group.step,
          group.orientation,
          isReversed(event.currentTarget, group.orientation),
        );
        if (amount != null) {
          event.preventDefault();
          moveBy(amount);
        }
      })}
      role="separator"
      // A separator that is not in the tab sequence is the WCAG 2.1.1 failure
      // this module exists to avoid.
      tabIndex={group.disabled ? -1 : 0}
    />
  );
}

/**
 * How far a key moves the splitter, or nothing when the key is not ours.
 *
 * Only the keys along the axis the panes are laid out on: `ArrowUp` in a group
 * of side-by-side panes is the page's, and swallowing it takes a scroll key
 * away from every reader who uses one.
 *
 * The primary pane is the one before the handle, so moving the handle towards
 * the end of the axis makes it larger — and in a right-to-left page the end of
 * a horizontal axis is on the left, so the arrows mirror.
 */
function stepFor(
  key: string,
  step: number,
  orientation: Orientation,
  reversed: boolean,
): number | null {
  const move = step <= 0 ? 1 : step;
  if (orientation === "vertical") {
    return match (key) {
      "ArrowDown" => move,
      "ArrowUp" => -move,
      _ => null,
    };
  }
  const forward = reversed ? -1 : 1;
  return match (key) {
    "ArrowRight" => move * forward,
    "ArrowLeft" => -move * forward,
    _ => null,
  };
}
