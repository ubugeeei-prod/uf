// @flow
//
// Typography: headings, paragraphs, a quotation, a list and inline code, on
// one type scale, each part the element its name says.
//
// `uf ui add typography` wrote this file into the project, and it is the
// project's from then on. `uf ui diff typography` shows how it has moved away
// from the registry in the uf you are running.
//
// # Why a component here, when `@uniflowed/ui` has none
//
// Every part is one of the platform's own elements, and the only decision in
// any of them is which element a caller picks, which a component cannot make
// for them. So `@uniflowed/ui` declines a `Typography` (`crates/uf_lib/src/ui.rs`
// records it), and `textStyles` in `@uniflowed/stylex/preset` is the class list
// on its own. What an application wants from this file is prose that looks the
// same on every page: a changelog, a help article, an empty settings screen.
//
// # What to keep true when you change it
//
// * **Each part is its element.** `H2` is an `<h2>`, `List` is a `<ul>` (or an
//   `<ol>` when `ordered`), `Blockquote` is a `<blockquote>` and `InlineCode`
//   is a `<code>`. Screen reader users move through a page by its headings and
//   hear a list's length before its items, and neither works when the element
//   is a styled `<div>`.
// * **Pick the heading by its level, not its size.** The level is the outline:
//   an `H3` belongs under an `H2`. A heading that should look smaller than its
//   level takes an `xstyle`, rather than skipping a level to get the size.
// * **A list keeps its markers.** Bullets and numbers are how a sighted
//   reader sees that the lines are one list and in what order. A list of rows
//   with no markers is `item.js`'s `Item.Group`.
// * **`Lead` and `Muted` are paragraphs.** They are quieter or larger text,
//   not headings, and are read as prose.
// * **Text stays on measured pairs.** `ink` and `muted` on `canvas` and
//   `surface`, and `ink` on `sunken` for code, which
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both shipped
//   themes.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

/**
 * Every prop a caller passes that this file does not name, on its way to the
 * element. `key` is `empty` because React takes it off before a component is
 * called.
 */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  heading: {
    margin: 0,
    fontFamily: ufTokens.fontSans,
    fontWeight: ufTokens.weightBold,
    lineHeight: ufTokens.leadingTight,
    color: ufTokens.ink,
    textWrap: "balance",
  },
  h1: {
    fontSize: ufTokens.text2Xl,
  },
  h2: {
    paddingBlockEnd: ufTokens.space2,
    fontSize: ufTokens.textXl,
    borderBlockEndWidth: "1px",
    borderBlockEndStyle: "solid",
    borderBlockEndColor: ufTokens.border,
  },
  h3: {
    fontSize: ufTokens.textLg,
  },
  h4: {
    fontSize: ufTokens.textMd,
  },
  prose: {
    margin: 0,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textMd,
    lineHeight: ufTokens.leadingBase,
    color: ufTokens.ink,
  },
  lead: {
    fontSize: ufTokens.textLg,
    color: ufTokens.muted,
  },
  muted: {
    fontSize: ufTokens.textSm,
    color: ufTokens.muted,
  },
  blockquote: {
    paddingInlineStart: ufTokens.space4,
    fontStyle: "italic",
    borderInlineStartWidth: "2px",
    borderInlineStartStyle: "solid",
    borderInlineStartColor: ufTokens.border,
  },
  list: {
    display: "grid",
    gap: ufTokens.space2,
    paddingInlineStart: ufTokens.space6,
  },
  code: {
    paddingBlock: "1px",
    paddingInline: ufTokens.space1,
    fontFamily: ufTokens.fontMono,
    fontSize: "0.875em",
    color: ufTokens.ink,
    backgroundColor: ufTokens.sunken,
    borderRadius: ufTokens.radiusSm,
  },
});

/** A first-level heading: the page's title, once. */
component TypographyH1(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <h1
      {...rest}
      className={classNames(props(styles.heading, styles.h1, xstyle).className, className)}
    >
      {children}
    </h1>
  );
}

/** A second-level heading, ruled underneath: a section of the page. */
component TypographyH2(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <h2
      {...rest}
      className={classNames(props(styles.heading, styles.h2, xstyle).className, className)}
    >
      {children}
    </h2>
  );
}

/** A third-level heading: a part of a section. */
component TypographyH3(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <h3
      {...rest}
      className={classNames(props(styles.heading, styles.h3, xstyle).className, className)}
    >
      {children}
    </h3>
  );
}

/** A fourth-level heading. */
component TypographyH4(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <h4
      {...rest}
      className={classNames(props(styles.heading, styles.h4, xstyle).className, className)}
    >
      {children}
    </h4>
  );
}

/** A paragraph of body text. */
component TypographyP(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <p {...rest} className={classNames(props(styles.prose, xstyle).className, className)}>
      {children}
    </p>
  );
}

/** The paragraph that opens a page, larger and quieter than the rest. */
component TypographyLead(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <p
      {...rest}
      className={classNames(props(styles.prose, styles.lead, xstyle).className, className)}
    >
      {children}
    </p>
  );
}

/** A paragraph of secondary text: a note, a date, a caption. */
component TypographyMuted(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <p
      {...rest}
      className={classNames(props(styles.prose, styles.muted, xstyle).className, className)}
    >
      {children}
    </p>
  );
}

/** A quotation from somewhere else. `cite` takes the URL it came from. */
component TypographyBlockquote(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <blockquote
      {...rest}
      className={classNames(props(styles.prose, styles.blockquote, xstyle).className, className)}
    >
      {children}
    </blockquote>
  );
}

/** A list of `<li>`s: bulleted, or numbered when `ordered`. */
component TypographyList(
  children: React.Node,
  ordered?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const shared = {
    ...rest,
    className: classNames(props(styles.prose, styles.list, xstyle).className, className),
  };
  return ordered ? <ol {...shared}>{children}</ol> : <ul {...shared}>{children}</ul>;
}

/** Code inside a sentence: a name, a command, a value. */
component TypographyInlineCode(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <code {...rest} className={classNames(props(styles.code, xstyle).className, className)}>
      {children}
    </code>
  );
}

/** The classes this file chose, then the caller's. */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}

/**
 * The parts, under the names `import * as Typography from "./typography.js"`
 * gives them.
 *
 * One name per component (ubugeeei-prod/uf#1453): a page writes
 * `<Typography.H2>` and `<Typography.P>`. Each is declared under its full name,
 * so React DevTools and an error say `TypographyH2` rather than `H2`.
 */
export {
  TypographyH1 as H1,
  TypographyH2 as H2,
  TypographyH3 as H3,
  TypographyH4 as H4,
  TypographyP as P,
  TypographyLead as Lead,
  TypographyMuted as Muted,
  TypographyBlockquote as Blockquote,
  TypographyList as List,
  TypographyInlineCode as InlineCode,
};
