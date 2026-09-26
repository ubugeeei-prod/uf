// @flow
//
// Separator: a line between groups of content, announced when it marks a
// boundary and silent when it is only a line.
//
// `uf ui add separator` wrote this file into the project, and it is the
// project's from then on. `uf ui diff separator` shows how it has moved away
// from the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The line: its colour, thickness, length and the space around it.
// `@uniflowed/ui`'s `Separator` owns the one decision in it: `role="separator"`
// with an `aria-orientation` for a boundary a reader should be told about, and
// `aria-hidden` for a rule that is only decoration.
//
// # What to keep true when you change it
//
// * **Say which it is.** `decorative` for a rule that is only visual. Left off,
//   a reader is told the content changes here.
// * **A vertical separator takes its height from its row.** It stretches to the
//   flex row it sits in, and a row with no height draws nothing.
// * **It carries no text,** so `border` has no contrast pair to keep; a line a
//   reader needs in order to understand the page is not decorative.

import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import { Separator as SeparatorPart } from "@uniflowed/ui";

/** Which way the line runs. */
export type SeparatorOrientation = "horizontal" | "vertical";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  base: {
    flexShrink: 0,
    margin: 0,
    borderWidth: 0,
    backgroundColor: ufTokens.border,
  },
  horizontal: {
    width: "100%",
    height: "1px",
    marginBlock: ufTokens.space4,
  },
  vertical: {
    width: "1px",
    alignSelf: "stretch",
    marginInline: ufTokens.space2,
  },
});

/** A line between groups of content. */
export component Separator(
  decorative?: boolean = false,
  orientation?: SeparatorOrientation = "horizontal",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const styled = props(
    styles.base,
    match (orientation) {
      "horizontal" => styles.horizontal,
      "vertical" => styles.vertical,
    },
    xstyle,
  );
  return (
    <SeparatorPart
      {...forwarded(rest)}
      className={classNames(styled.className, className)}
      decorative={decorative}
      orientation={orientation}
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
