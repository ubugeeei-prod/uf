// @flow
//
// Card: a surface that groups one thing's content, with an optional header,
// body and footer.
//
// `uf ui add card` wrote this file into the project, and it is the project's
// from then on. `uf ui diff card` shows how it has moved away from the
// registry in the uf you are running.
//
// # Why a component here, when `@uniflowed/ui` has none
//
// Each part of a card is a `<div>` with a class. A card has no role:
// `role="region"` needs a name, and naming every card on a page fills the
// landmark list with noise. So `@uniflowed/ui` declines a Card
// (`crates/uf_lib/src/ui.rs` says so), and the only semantics a card has come
// from its title, which is a real heading. What an application wants from
// this file is one card that all its pages agree on.
//
// # What to keep true when you change it
//
// * **The title is a heading at the level the page needs.** `Card.Title` takes
//   `level`, which is 3 by default and typed as 1 to 6, so a level that does
//   not exist fails `uf check` instead of rendering a `<div>`. Screen reader
//   users move through a page by its headings. A card title styled to look like
//   a heading, but not marked up as one, cannot be reached that way.
// * **A card that goes somewhere has one link, on its title.** Wrapping the
//   whole card in `<a>` makes a screen reader read every word in the card as the
//   link's name. It also cannot hold a second control. Put the link in
//   `Card.Title`, and stretch its hit area with CSS if the whole card should
//   respond to a click.
// * **Content order is reading order.** The footer's actions come after the
//   content in the DOM, so a keyboard reaches them after reading what they act
//   on.
// * **Text stays on measured pairs.** `ink` and `muted` on `surface`, which
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both shipped
//   themes.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

/** A heading level, which is all a card title can be. */
export type CardTitleLevel = 1 | 2 | 3 | 4 | 5 | 6;

/**
 * Every prop a caller passes that this file does not name, on its way to the
 * element. `key` is `empty` because React takes it off before a component is
 * called.
 */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  card: {
    display: "grid",
    gap: ufTokens.space4,
    boxSizing: "border-box",
    paddingBlock: ufTokens.space6,
    backgroundColor: ufTokens.surface,
    color: ufTokens.ink,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingBase,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusLg,
  },
  header: {
    display: "grid",
    gap: ufTokens.space1,
    paddingInline: ufTokens.space6,
  },
  title: {
    margin: 0,
    fontSize: ufTokens.textMd,
    fontWeight: ufTokens.weightBold,
    lineHeight: ufTokens.leadingTight,
  },
  description: {
    margin: 0,
    color: ufTokens.muted,
  },
  content: {
    paddingInline: ufTokens.space6,
  },
  footer: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: ufTokens.space2,
    paddingInline: ufTokens.space6,
  },
});

/** The surface. */
component CardRoot(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <div {...rest} className={classNames(props(styles.card, xstyle).className, className)}>
      {children}
    </div>
  );
}

/** The title, the description under it, and anything that sits beside them. */
component CardHeader(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <div {...rest} className={classNames(props(styles.header, xstyle).className, className)}>
      {children}
    </div>
  );
}

/** What the card is about, as a heading at `level`. */
component CardTitle(
  children: React.Node,
  level?: CardTitleLevel = 3,
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

/** A line under the title, in the quieter colour. */
component CardDescription(
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

/** The body. */
component CardContent(
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

/** The actions, after the content they act on. */
component CardFooter(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <div {...rest} className={classNames(props(styles.footer, xstyle).className, className)}>
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
 * The parts, under the names `import * as Card from "./card.js"` gives them.
 *
 * One name per component (ubugeeei-prod/uf#1453): a page writes `<Card.Root>`
 * and `<Card.Header>`, the way it writes `@uniflowed/ui`'s own parts. Each
 * is declared under its full name, so React DevTools and an error say
 * `CardRoot` rather than `Root`.
 */
export {
  CardRoot as Root,
  CardHeader as Header,
  CardTitle as Title,
  CardDescription as Description,
  CardContent as Content,
  CardFooter as Footer,
};
