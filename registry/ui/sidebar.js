"use client";
// @flow
//
// Sidebar: a column of navigation beside the page, collapsible to icons on a
// wide screen and a sheet that slides in on a narrow one.
//
// `uf ui add sidebar` wrote this file into the project, and it is the project's
// from then on. `uf ui diff sidebar` shows how it has moved away from the
// registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The layout beside the page, the column's width open and collapsed, its
// header and footer, the items with their icons, and the button that opens and
// collapses it. `@uniflowed/ui`'s `Sidebar` owns the rest: a named `<nav>`, a
// button that says with `aria-expanded` whether the column is open and names
// what it controls, a sheet in place of the column on a narrow screen, and an
// item that keeps its name while collapsed to an icon, with a tooltip saying
// it. The column's width and an item's label are drawn from `data-collapsed`,
// and the current page's item from `aria-current`.
//
// # What to keep true when you change it
//
// * **Name the navigation.** `label` on `SidebarContent` says which navigation
//   this is, such as "Main".
// * **Give every item a `label`.** It is the item's name while only its icon
//   shows.
// * **Mark where the reader is.** `aria-current="page"` on the current item, so
//   a reader hears it, not only sees it.
// * **Text stays on measured pairs.** `ink` on `surface`, and `accent` on
//   `accentSoft` for the current item, which
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both themes.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import type { SidebarSide } from "@uniflowed/ui";
import * as Primitive from "@uniflowed/ui";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

/** A caller's own element in place of the one a part renders. */
type RenderProp = (props: Rest) => React.Node;

const styles = stylex.create({
  layout: {
    display: "flex",
    boxSizing: "border-box",
    minHeight: "100%",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
  },
  content: {
    display: "flex",
    flexDirection: "column",
    flexShrink: 0,
    gap: ufTokens.space1,
    boxSizing: "border-box",
    width: { default: "16rem", ":is([data-collapsed=true])": "3.5rem" },
    height: "100%",
    padding: ufTokens.space2,
    overflowY: "auto",
    backgroundColor: ufTokens.surface,
    borderInlineEndWidth: "1px",
    borderInlineEndStyle: "solid",
    borderInlineEndColor: ufTokens.border,
  },
  panel: {
    position: "fixed",
    zIndex: 50,
    boxSizing: "border-box",
    display: "flex",
    flexDirection: "column",
    top: 0,
    bottom: 0,
    width: "min(20rem, calc(100% - 48px))",
    padding: 0,
    overflowY: "auto",
    backgroundColor: ufTokens.surface,
    color: ufTokens.ink,
    boxShadow: ufTokens.shadowPanel,
    borderWidth: 0,
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderLeftWidth: { default: 0, ":is([data-side=right])": "1px" },
    borderRightWidth: { default: 0, ":is([data-side=left])": "1px" },
    left: { default: 0, ":is([data-side=right])": "auto" },
    right: { default: 0, ":is([data-side=left])": "auto" },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "-2px",
  },
  header: {
    display: "flex",
    alignItems: "center",
    gap: ufTokens.space2,
    minHeight: "40px",
    paddingInline: ufTokens.space2,
    fontWeight: ufTokens.weightMedium,
  },
  footer: {
    marginBlockStart: "auto",
    paddingInline: ufTokens.space2,
    paddingBlock: ufTokens.space2,
  },
  item: {
    // Read by the label inside, which cannot see whether the column is collapsed.
    "--uf-sidebar-label": { default: "inline", ":is([data-collapsed=true])": "none" },
    display: "flex",
    alignItems: "center",
    gap: ufTokens.space2,
    boxSizing: "border-box",
    width: "100%",
    minHeight: "36px",
    margin: 0,
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space2,
    borderWidth: 0,
    borderRadius: ufTokens.radiusSm,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingTight,
    textAlign: "start",
    textDecoration: "none",
    cursor: "pointer",
    color: {
      default: ufTokens.ink,
      ":hover": ufTokens.accent,
      ":is([aria-current=page])": ufTokens.accent,
    },
    backgroundColor: {
      default: "transparent",
      ":hover": ufTokens.accentSoft,
      ":is([aria-current=page])": ufTokens.accentSoft,
    },
    fontWeight: {
      default: ufTokens.weightRegular,
      ":is([aria-current=page])": ufTokens.weightMedium,
    },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "-2px",
  },
  icon: {
    display: "inline-flex",
    flexShrink: 0,
    width: "20px",
    justifyContent: "center",
  },
  label: {
    display: "var(--uf-sidebar-label, inline)",
    overflow: "hidden",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
  },
  trigger: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: "36px",
    height: "36px",
    margin: 0,
    padding: 0,
    color: { default: ufTokens.ink, ":hover": ufTokens.accent },
    backgroundColor: { default: "transparent", ":hover": ufTokens.accentSoft },
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusSm,
    cursor: "pointer",
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
  tooltip: {
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
    boxShadow: ufTokens.shadowCard,
  },
});

/**
 * The sidebar and the page beside it. Open unless `defaultOpen` or `open` says
 * otherwise; `narrowQuery` is the media query under which it is a sheet.
 */
export component Sidebar(
  children: React.Node,
  defaultOpen?: boolean = true,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
  side?: SidebarSide = "left",
  narrowQuery?: string,
  xstyle?: StyleArgument,
  className?: string,
) {
  return (
    <Primitive.SidebarRoot
      defaultOpen={defaultOpen}
      narrowQuery={narrowQuery}
      onOpenChange={onOpenChange}
      open={open}
      side={side}
    >
      <div className={classNames(props(styles.layout, xstyle).className, className)}>
        {children}
      </div>
    </Primitive.SidebarRoot>
  );
}

/** The column itself, a `<nav>` named by `label`. */
export component SidebarContent(
  children: React.Node,
  label: string,
  panelXstyle?: StyleArgument,
  panelClassName?: string,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.SidebarBody
      {...forwarded(rest)}
      className={classNames(props(styles.content, xstyle).className, className)}
      label={label}
      sheetProps={{
        className: classNames(props(styles.panel, panelXstyle).className, panelClassName),
      }}
    >
      {children}
    </Primitive.SidebarBody>
  );
}

/** The top of the column, such as the product's name. */
export component SidebarHeader(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.SidebarHeader
      {...forwarded(rest)}
      className={classNames(props(styles.header, xstyle).className, className)}
    >
      {children}
    </Primitive.SidebarHeader>
  );
}

/** The bottom of the column, such as the signed-in account. */
export component SidebarFooter(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.SidebarFooter
      {...forwarded(rest)}
      className={classNames(props(styles.footer, xstyle).className, className)}
    >
      {children}
    </Primitive.SidebarFooter>
  );
}

/**
 * One item: an icon and its words, and only the icon while the column is
 * collapsed. It is a button unless `render` gives it another element, such as
 * a link.
 */
export component SidebarItem(
  children: React.Node,
  label: string,
  icon?: React.Node,
  render?: RenderProp,
  tooltipXstyle?: StyleArgument,
  tooltipClassName?: string,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.SidebarItem
      {...forwarded(rest)}
      className={classNames(props(styles.item, xstyle).className, className)}
      label={label}
      render={render}
      tooltipProps={{
        className: classNames(props(styles.tooltip, tooltipXstyle).className, tooltipClassName),
      }}
    >
      {icon == null ? null : (
        <span {...props(styles.icon)} aria-hidden="true">
          {icon}
        </span>
      )}
      <span {...props(styles.label)}>{children}</span>
    </Primitive.SidebarItem>
  );
}

/** The button that opens and collapses the column. `label` says what it does. */
export component SidebarTrigger(
  label?: string = "Toggle sidebar",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.SidebarTrigger
      {...forwarded(rest)}
      aria-label={label}
      className={classNames(props(styles.trigger, xstyle).className, className)}
    >
      <svg
        aria-hidden="true"
        fill="none"
        focusable="false"
        height="16"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="2"
        viewBox="0 0 24 24"
        width="16"
      >
        <rect height="18" rx="2" width="18" x="3" y="3" />
        <path d="M9 3v18" />
      </svg>
    </Primitive.SidebarTrigger>
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
