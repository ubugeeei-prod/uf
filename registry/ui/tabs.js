"use client";
// @flow
//
// Tabs: `@uniflowed/ui`'s tab set, with the selected tab underlined.
//
// `uf ui add tabs` wrote this file into the project, and it is the project's
// from then on. `uf ui diff tabs` shows how it has moved away from the
// registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The look: the row, the underline under the selected tab, the dimmed disabled
// tab, the focus rings. The behaviour is imported from `@uniflowed/ui`'s `Tabs`:
// one tab stop for the whole row, arrow keys that move and wrap, `Home` and
// `End`, automatic or manual activation, and a panel that points back at the
// tab that shows it. A fix to any of that reaches this project by upgrading
// `@uniflowed/ui`.
//
// The selected look follows `aria-selected`, the attribute the part already
// writes, through the condition `:is([aria-selected=true])`. Nothing here
// tracks which tab is selected, so an arrow key, a click and a controlled
// `value` all draw the same underline without this file hearing about any of
// them.
//
// # What to keep true when you change it
//
// * **A tab list holds tabs and nothing else.** `TabsList` takes
//   `renders* TabsTab`, and `TabsTab` renders `@uniflowed/ui`'s tab, so a
//   `<button>` dropped into the row is a Flow error here exactly as it is in
//   the package. The constraint survives the copy.
// * **Name the list.** `aria-label` on `TabsList` says what the tabs choose
//   between; without it a reader hears "tab list" and no subject.
// * **Selected is drawn twice.** The underline, and the text going from `muted`
//   to `ink`, because colour alone does not tell a reader which tab is chosen
//   (WCAG 1.4.1) and a two-pixel line alone is easy to miss.
// * **A disabled tab stays in the row.** It is `aria-disabled`, dimmed and
//   passed over by the arrow keys rather than removed, so a reader learns the
//   section exists and is unavailable.
// * **The panel is focusable, and shows it.** `Tab` out of the row lands on
//   the panel, and the ring says where focus went.
// * **Colour comes from tokens, in measured pairs:** `ink` and `muted` on
//   `canvas` and `surface`, which `crates/uf_stylex/src/tests/preset.rs` holds
//   to 4.5:1 in the light default and the dark theme.

import * as React from "@uniflowed/react";
import { createContext, useContext } from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import type { ActivationMode } from "@uniflowed/ui";
import * as Primitive from "@uniflowed/ui";

/** Which way the row runs, and so which arrow keys move along it. */
export type TabsOrientation = "horizontal" | "vertical";

/**
 * Every prop a caller passes that this file does not name, on its way to the
 * part underneath. See `forwarded` for the one thing Flow cannot say about it.
 */
type Rest = { readonly key?: empty, readonly [string]: mixed };

/**
 * The orientation the caller gave `Tabs`, for the parts that draw differently
 * along each axis. This is the caller's own prop handed down, not state the
 * part owns, so there is nothing here that could disagree with it.
 */
const OrientationContext: React.Context<TabsOrientation> = createContext("horizontal");

const styles = stylex.create({
  root: {
    display: "flex",
    flexDirection: "column",
    gap: ufTokens.space4,
    fontFamily: ufTokens.fontSans,
    color: ufTokens.ink,
  },
  rootVertical: {
    flexDirection: "row",
  },
  list: {
    display: "flex",
    gap: ufTokens.space1,
    borderBottomWidth: "1px",
    borderBottomStyle: "solid",
    borderBottomColor: ufTokens.border,
  },
  listVertical: {
    flexDirection: "column",
    borderBottomWidth: 0,
    borderInlineEndWidth: "1px",
    borderInlineEndStyle: "solid",
    borderInlineEndColor: ufTokens.border,
  },
  tab: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    gap: ufTokens.space2,
    boxSizing: "border-box",
    minHeight: "36px",
    margin: 0,
    // Over the list's own border, so the underline replaces that line under
    // the selected tab rather than sitting on top of it.
    marginBottom: "-1px",
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space3,
    borderWidth: 0,
    borderStyle: "solid",
    borderColor: "transparent",
    borderBottomWidth: "2px",
    borderBottomColor: {
      default: "transparent",
      ":is([aria-selected=true])": ufTokens.accent,
    },
    backgroundColor: "transparent",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    fontWeight: ufTokens.weightMedium,
    lineHeight: ufTokens.leadingTight,
    color: {
      default: ufTokens.muted,
      ":hover": ufTokens.ink,
      ":is([aria-selected=true])": ufTokens.ink,
    },
    cursor: { default: "pointer", ":is([aria-disabled=true])": "not-allowed" },
    opacity: { default: 1, ":is([aria-disabled=true])": 0.55 },
    // Inside the tab, so the list's edge never clips the ring.
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "-2px",
    // The underline and the label change together in `durationBase`: long
    // enough to see which tab took the mark from which. Colours, so they stay
    // under reduced motion; the ring does not.
    transitionProperty: {
      default: "border-color, color, outline-width",
      "@media (prefers-reduced-motion: reduce)": "border-color, color",
    },
    transitionDuration: ufTokens.durationBase,
    transitionTimingFunction: ufTokens.easing,
  },
  tabVertical: {
    justifyContent: "flex-start",
    marginBottom: 0,
    marginInlineEnd: "-1px",
    borderBottomWidth: 0,
    borderInlineEndWidth: "2px",
    borderInlineEndColor: {
      default: "transparent",
      ":is([aria-selected=true])": ufTokens.accent,
    },
  },
  panel: {
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingBase,
    borderRadius: ufTokens.radiusSm,
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
});

/**
 * The tab set. Uncontrolled from `defaultValue` unless `value` is given.
 *
 *     <Tabs defaultValue="overview">
 *       <TabsList aria-label="Project">
 *         <TabsTab value="overview">Overview</TabsTab>
 *         <TabsTab value="activity">Activity</TabsTab>
 *       </TabsList>
 *       <TabsPanel value="overview">…</TabsPanel>
 *       <TabsPanel value="activity">…</TabsPanel>
 *     </Tabs>
 */
export component Tabs(
  children: React.Node,
  defaultValue: string,
  value?: string,
  onValueChange?: (value: string) => void,
  activationMode?: ActivationMode = "automatic",
  orientation?: TabsOrientation = "horizontal",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const styled = props(styles.root, orientation === "vertical" && styles.rootVertical, xstyle);
  return (
    <OrientationContext.Provider value={orientation}>
      <Primitive.Tabs.Root
        {...forwarded(rest)}
        activationMode={activationMode}
        className={classNames(styled.className, className)}
        defaultValue={defaultValue}
        onValueChange={onValueChange}
        orientation={orientation}
        value={value}
      >
        {children}
      </Primitive.Tabs.Root>
    </OrientationContext.Provider>
  );
}

/** The row of tabs. Give it an `aria-label` that says what they choose between. */
export component TabsList(
  children: renders* TabsTab,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const orientation = useContext(OrientationContext);
  const styled = props(styles.list, orientation === "vertical" && styles.listVertical, xstyle);
  return (
    <Primitive.Tabs.List {...forwarded(rest)} className={classNames(styled.className, className)}>
      {children}
    </Primitive.Tabs.List>
  );
}

/** One tab. `disabled` keeps it in the row, announced and unavailable. */
export component TabsTab(
  value: string,
  children: React.Node,
  disabled?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.Tabs.Tab {
  const orientation = useContext(OrientationContext);
  const styled = props(styles.tab, orientation === "vertical" && styles.tabVertical, xstyle);
  return (
    <Primitive.Tabs.Tab
      {...forwarded(rest)}
      className={classNames(styled.className, className)}
      disabled={disabled}
      value={value}
    >
      {children}
    </Primitive.Tabs.Tab>
  );
}

/** The panel a tab shows, in the document only while its tab is selected. */
export component TabsPanel(
  value: string,
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.Tabs.Panel
      {...forwarded(rest)}
      className={classNames(props(styles.panel, xstyle).className, className)}
      value={value}
    >
      {children}
    </Primitive.Tabs.Panel>
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
