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
// The underline slides. `Tabs.List` writes where the selected tab is as four
// unitless custom properties (`--uf-tabs-indicator-left`, `-top`, `-width`,
// `-height`), and one bar in the list follows them with `transform` alone: a
// one-pixel bar moved by `translateX` and stretched to the tab by `scaleX`, so
// nothing is laid out while it travels. Until the part has measured — on the
// server, and before hydration — each selected tab draws its own underline
// instead, so the selection is never shown by colour alone.
//
// # What to keep true when you change it
//
// * **A tab list holds tabs and nothing else.** `Tabs.List` takes
//   `renders* TabsTab`, and `Tabs.Tab` renders `@uniflowed/ui`'s tab, so a
//   `<button>` dropped into the row is a Flow error here exactly as it is in
//   the package. The constraint survives the copy.
// * **Name the list.** `aria-label` on `Tabs.List` says what the tabs choose
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
import { createContext, useContext, useEffect, useState } from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import type { ActivationMode } from "@uniflowed/ui";
import { Tabs } from "@uniflowed/ui";

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

/**
 * Whether the list's sliding bar is drawn, so a tab stops drawing its own
 * underline. False on the server and until the list has been measured.
 */
const SlidingContext: React.Context<boolean> = createContext(false);

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
    // The containing block the sliding bar is placed in.
    position: "relative",
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
  // Once the bar is drawn, the tab's own underline goes: the bar is the mark.
  tabSliding: {
    borderBottomColor: "transparent",
  },
  tabSlidingVertical: {
    borderInlineEndColor: "transparent",
  },
  // The bar: one pixel along the row, moved to the selected tab and stretched
  // to its width by the part's measurements, over the list's own border line
  // where a tab's underline would be. `transform` only, so it slides without
  // laying anything out; `durationBase` on the standard curve, because it is
  // travelling between two places that are both on screen, not arriving.
  // `scaleX` here is the tab's width in pixels, geometry rather than a grow.
  // It fades in when it is first drawn. Under reduced motion it moves at once.
  indicator: {
    position: "absolute",
    left: 0,
    bottom: "-1px",
    width: "1px",
    height: "2px",
    backgroundColor: ufTokens.accent,
    pointerEvents: "none",
    transformOrigin: "0 0",
    transform:
      "translateX(calc(var(--uf-tabs-indicator-left, 0) * 1px)) scaleX(var(--uf-tabs-indicator-width, 0))",
    opacity: { default: 1, "@starting-style": 0 },
    transitionProperty: {
      default: "transform, opacity",
      "@media (prefers-reduced-motion: reduce)": "opacity",
    },
    transitionDuration: ufTokens.durationBase,
    transitionTimingFunction: ufTokens.easing,
  },
  // Down the inline-end edge of a vertical list, where its border is.
  indicatorVertical: {
    left: "auto",
    bottom: "auto",
    top: 0,
    insetInlineEnd: "-1px",
    width: "2px",
    height: "1px",
    transform:
      "translateY(calc(var(--uf-tabs-indicator-top, 0) * 1px)) scaleY(var(--uf-tabs-indicator-height, 0))",
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
component TabsRoot(
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
      <Tabs.Root
        {...forwarded(rest)}
        activationMode={activationMode}
        className={classNames(styled.className, className)}
        defaultValue={defaultValue}
        onValueChange={onValueChange}
        orientation={orientation}
        value={value}
      >
        {children}
      </Tabs.Root>
    </OrientationContext.Provider>
  );
}

/** The row of tabs. Give it an `aria-label` that says what they choose between. */
component TabsList(
  children: renders* TabsTab,
  xstyle?: StyleArgument,
  className?: string,
  ...given: Rest
) {
  const orientation = useContext(OrientationContext);
  const vertical = orientation === "vertical";
  const styled = props(styles.list, orientation === "vertical" && styles.listVertical, xstyle);
  // The sliding bar, drawn from the commit after the first, by which time
  // `Tabs.List` has written where the selected tab is: a bar drawn before
  // that would slide in from the list's corner on every page load. It goes
  // after the tabs, through the part's `render`, and tells them through
  // `SlidingContext` that they need not underline themselves.
  const [sliding, setSliding] = useState(false);
  useEffect(() => {
    // uf-lint-disable-next-line react-compiler/set-state-in-effect
    setSliding(true);
  }, []);
  const rest: Rest = {
    ...given,
    render: (list: Rest) => (
      <SlidingContext.Provider value={sliding}>
        <div {...forwarded(list)}>
          {children}
          {sliding ? (
            <span
              {...props(styles.indicator, vertical && styles.indicatorVertical)}
              aria-hidden="true"
            />
          ) : null}
        </div>
      </SlidingContext.Provider>
    ),
  };

  return (
    <Tabs.List {...forwarded(rest)} className={classNames(styled.className, className)}>
      {children}
    </Tabs.List>
  );
}

/** One tab. `disabled` keeps it in the row, announced and unavailable. */
component TabsTab(
  value: string,
  children: React.Node,
  disabled?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Tabs.Tab {
  const orientation = useContext(OrientationContext);
  const sliding = useContext(SlidingContext);
  const vertical = orientation === "vertical";
  const styled = props(
    styles.tab,
    vertical && styles.tabVertical,
    sliding && (vertical ? styles.tabSlidingVertical : styles.tabSliding),
    xstyle,
  );
  return (
    <Tabs.Tab
      {...forwarded(rest)}
      className={classNames(styled.className, className)}
      disabled={disabled}
      value={value}
    >
      {children}
    </Tabs.Tab>
  );
}

/** The panel a tab shows, in the document only while its tab is selected. */
component TabsPanel(
  value: string,
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Tabs.Panel
      {...forwarded(rest)}
      className={classNames(props(styles.panel, xstyle).className, className)}
      value={value}
    >
      {children}
    </Tabs.Panel>
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
 * The parts, under the names `import * as Tabs from "./tabs.js"` gives them.
 *
 * One name per component (ubugeeei-prod/uf#1453): a page writes `<Tabs.Root>`
 * and `<Tabs.List>`, the way it writes `@uniflowed/ui`'s own parts. Each
 * is declared under its full name, so React DevTools and an error say
 * `TabsRoot` rather than `Root`.
 */
export { TabsRoot as Root, TabsList as List, TabsTab as Tab, TabsPanel as Panel };
