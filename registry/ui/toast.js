"use client";
// @flow
//
// Toast: a short notice that appears in a corner of the page and goes away on
// its own or when dismissed, without taking focus from what the reader is doing.
//
// `uf ui add toast` wrote this file into the project, and it is the project's
// from then on. `uf ui diff toast` shows how it has moved away from the
// registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The corner the notices stack in, each notice's card, its title and
// description type, and its buttons. `@uniflowed/ui`'s `Toast` owns the rest: a
// `toast()` a page calls from anywhere, a named `role="region"` that `F6`
// reaches without joining the tab order, a polite live region for ordinary
// notices and an assertive one for urgent ones, a limit on how many show at
// once, each notice a `role="group"` named by its title and described by its
// description, and dismissal from its action and its close button.
//
// # What to keep true when you change it
//
// * **Render `Toast.Region` once.** Notices raised with `toast()` show wherever a
//   `Toast.Region` is rendered, so two would show every notice twice.
// * **Nothing only a toast can do.** A notice can go before a reader reaches it,
//   so what its action does must also be reachable somewhere else on the page.
// * **Name the close button for what it closes** when the notice's title does
//   not make it obvious: `Toast.Close` says "Dismiss" unless told otherwise.
// * **Text stays on measured pairs.** `ink` and `muted` on `surface`, which
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both themes.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import { Toast } from "@uniflowed/ui";

export { dismissAllToasts, dismissToast, toast, updateToast } from "@uniflowed/ui";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  region: {
    position: "fixed",
    zIndex: 100,
    insetBlockEnd: ufTokens.space4,
    insetInlineEnd: ufTokens.space4,
    boxSizing: "border-box",
    width: "min(24rem, calc(100vw - 32px))",
    margin: 0,
    padding: 0,
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "4px",
  },
  toast: {
    display: "grid",
    gridTemplateColumns: "1fr auto",
    alignItems: "start",
    columnGap: ufTokens.space3,
    rowGap: ufTokens.space1,
    boxSizing: "border-box",
    width: "100%",
    marginBlockStart: ufTokens.space2,
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
    // Enter: it rises 16px into its place in the stack as it fades in, over
    // `durationSlow`, so a notification arriving in a corner is noticed
    // without being startling. Vertical, so it reads the same in a
    // right-to-left page. Under reduced motion it only fades.
    opacity: { default: 1, "@starting-style": 0 },
    transform: { default: "none", "@starting-style": "translateY(16px)" },
    transitionProperty: {
      default: "opacity, transform",
      "@media (prefers-reduced-motion: reduce)": "opacity",
    },
    transitionDuration: ufTokens.durationSlow,
    transitionTimingFunction: ufTokens.easingEnter,
  },
  title: {
    gridColumn: "1",
    fontWeight: ufTokens.weightMedium,
  },
  description: {
    gridColumn: "1",
    color: ufTokens.muted,
  },
  action: {
    gridColumn: "1",
    justifySelf: "start",
    marginBlockStart: ufTokens.space2,
    minHeight: "32px",
    paddingBlock: ufTokens.space1,
    paddingInline: ufTokens.space3,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    fontWeight: ufTokens.weightMedium,
    color: ufTokens.ink,
    backgroundColor: ufTokens.surface,
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
  close: {
    gridColumn: "2",
    gridRow: "1",
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: "24px",
    height: "24px",
    margin: 0,
    padding: 0,
    color: { default: ufTokens.muted, ":hover": ufTokens.ink },
    backgroundColor: "transparent",
    borderWidth: 0,
    borderRadius: ufTokens.radiusSm,
    cursor: "pointer",
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "1px",
  },
});

/**
 * Where notices raised with `toast()` show: a stack in the page's bottom inline
 * end corner, each with a close button.
 */
component Toaster(
  label?: string = "Notifications",
  limit?: number = 3,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Toast.Region
      {...forwarded(rest)}
      className={classNames(props(styles.region, xstyle).className, className)}
      label={label}
      limit={limit}
    >
      {(notification) => (
        <ToastRoot>
          {notification.content}
          <ToastClose />
        </ToastRoot>
      )}
    </Toast.Region>
  );
}

/** One notice's card. `Toast.Region` draws one for every notice. */
component ToastRoot(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Toast.Root {
  return (
    <Toast.Root
      {...forwarded(rest)}
      className={classNames(props(styles.toast, xstyle).className, className)}
    >
      {children}
    </Toast.Root>
  );
}

/** What happened, in a few words. It names the notice. */
component ToastTitle(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Toast.Title
      {...forwarded(rest)}
      className={classNames(props(styles.title, xstyle).className, className)}
    >
      {children}
    </Toast.Title>
  );
}

/** The detail under the title. It describes the notice. */
component ToastDescription(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Toast.Description
      {...forwarded(rest)}
      className={classNames(props(styles.description, xstyle).className, className)}
    >
      {children}
    </Toast.Description>
  );
}

/** A button that acts on the notice and then dismisses it. */
component ToastAction(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Toast.Action
      {...forwarded(rest)}
      className={classNames(props(styles.action, xstyle).className, className)}
      type="button"
    >
      {children}
    </Toast.Action>
  );
}

/** The button in the corner that dismisses the notice. */
component ToastClose(
  label?: string = "Dismiss",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Toast.Close
      {...forwarded(rest)}
      className={classNames(props(styles.close, xstyle).className, className)}
      label={label}
      type="button"
    >
      <svg
        aria-hidden="true"
        fill="none"
        focusable="false"
        height="14"
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth="2"
        viewBox="0 0 24 24"
        width="14"
      >
        <path d="M18 6 6 18M6 6l12 12" />
      </svg>
    </Toast.Close>
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

/**
 * The parts, under the names `import * as Toast from "./toast.js"` gives them.
 *
 * One name per component (ubugeeei-prod/uf#1453): a page writes `<Toast.Region>`
 * and `<Toast.Root>`, the way it writes `@uniflowed/ui`'s own parts. Each
 * is declared under its full name, so React DevTools and an error say
 * `Toaster` rather than `Region`.
 */
export {
  Toaster as Region,
  ToastRoot as Root,
  ToastTitle as Title,
  ToastDescription as Description,
  ToastAction as Action,
  ToastClose as Close,
};
