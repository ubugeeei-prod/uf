// @flow
//
// Empty: what a list, a table or a page shows when it has nothing in it yet,
// and the action that fills it.
//
// `uf ui add empty` wrote this file into the project, and it is the project's
// from then on. `uf ui diff empty` shows how it has moved away from the
// registry in the uf you are running.
//
// # Why a component here, when `@uniflowed/ui` has none
//
// An empty state is a heading, a sentence and a button, and each is an
// element a caller already writes. It has no role of its own: it is rendered
// with the page rather than arriving later, so it is not a live region, and a
// `role="region"` would need a name and add one more landmark to a list that
// is useful only while it is short. `@uniflowed/ui` declines an `Empty` for
// that reason (`crates/uf_lib/src/ui.rs` records it). What an application wants
// is every empty screen looking and reading the same, so it is written here.
//
// # What to keep true when you change it
//
// * **The title is a heading at the level the page needs.** `Empty.Title`
//   takes `level`, 2 by default and typed as 1 to 6, because an empty state
//   usually stands where a section's content would be. A reader moving by
//   headings then finds it where the content would have been.
// * **Say what is empty and what to do next.** "No invoices yet" and a button
//   that creates one. A picture on its own tells a screen reader user nothing,
//   and a title with no next step leaves everybody stuck.
// * **The picture is decoration.** `Empty.Media` is `aria-hidden`, because the
//   title already says what it shows. Nothing inside it can be focused.
// * **When the empty state replaces results someone just asked for,** a search
//   that found nothing, announce it: render `Empty` inside the region that
//   was already announcing the results, or use `announce()` from
//   `@uniflowed/ui`. A panel that silently swaps its content is not heard.
// * **Text stays on measured pairs.** `ink` and `muted` on `surface` and
//   `canvas`, which `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in
//   both shipped themes.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

/** A heading level, which is all an empty state's title can be. */
export type EmptyTitleLevel = 1 | 2 | 3 | 4 | 5 | 6;

/**
 * Every prop a caller passes that this file does not name, on its way to the
 * element. `key` is `empty` because React takes it off before a component is
 * called.
 */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  root: {
    display: "grid",
    justifyItems: "center",
    gap: ufTokens.space4,
    boxSizing: "border-box",
    paddingBlock: ufTokens.space12,
    paddingInline: ufTokens.space6,
    textAlign: "center",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingBase,
    color: ufTokens.ink,
    borderWidth: "1px",
    borderStyle: "dashed",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusLg,
  },
  media: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: "40px",
    height: "40px",
    color: ufTokens.muted,
    backgroundColor: ufTokens.sunken,
    borderRadius: ufTokens.radiusMd,
  },
  title: {
    margin: 0,
    fontSize: ufTokens.textMd,
    fontWeight: ufTokens.weightBold,
    lineHeight: ufTokens.leadingTight,
  },
  description: {
    margin: 0,
    maxWidth: "32rem",
    color: ufTokens.muted,
  },
  content: {
    display: "flex",
    flexWrap: "wrap",
    justifyContent: "center",
    alignItems: "center",
    gap: ufTokens.space2,
  },
});

/** The frame. */
component EmptyRoot(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <div {...rest} className={classNames(props(styles.root, xstyle).className, className)}>
      {children}
    </div>
  );
}

/** An icon or an illustration, hidden from assistive technology. */
component EmptyMedia(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <div
      {...rest}
      aria-hidden="true"
      className={classNames(props(styles.media, xstyle).className, className)}
    >
      {children}
    </div>
  );
}

/** What is empty, as a heading at `level`. */
component EmptyTitle(
  children: React.Node,
  level?: EmptyTitleLevel = 2,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const shared = {
    ...rest,
    className: classNames(props(styles.title, xstyle).className, className),
  };
  return match (level) {
    1 => <h1 {...shared}>{children}</h1>,
    2 => <h2 {...shared}>{children}</h2>,
    3 => <h3 {...shared}>{children}</h3>,
    4 => <h4 {...shared}>{children}</h4>,
    5 => <h5 {...shared}>{children}</h5>,
    6 => <h6 {...shared}>{children}</h6>,
  };
}

/** Why it is empty, or what will appear here. */
component EmptyDescription(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <p {...rest} className={classNames(props(styles.description, xstyle).className, className)}>
      {children}
    </p>
  );
}

/** The next step: the button or link that fills it. */
component EmptyContent(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <div {...rest} className={classNames(props(styles.content, xstyle).className, className)}>
      {children}
    </div>
  );
}

/** The classes this file chose, then the caller's. */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}

/**
 * The parts, under the names `import * as Empty from "./empty.js"` gives them.
 *
 * One name per component (ubugeeei-prod/uf#1453): a page writes `<Empty.Root>`
 * and `<Empty.Title>`. Each is declared under its full name, so React DevTools
 * and an error say `EmptyRoot` rather than `Root`.
 */
export {
  EmptyRoot as Root,
  EmptyMedia as Media,
  EmptyTitle as Title,
  EmptyDescription as Description,
  EmptyContent as Content,
};
