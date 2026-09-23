// @flow
//
// Skeleton: the still shapes a page shows while its content loads, with the
// loading announced rather than only drawn.
//
// `uf ui add skeleton` wrote this file into the project, and it is the
// project's from then on. `uf ui diff skeleton` shows how it has moved away
// from the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The shapes: a line, a block and a circle in `sunken`. `@uniflowed/ui`'s `Skeleton`
// owns what a reader who cannot see them is told: every box is `aria-hidden`,
// the region they stand in is `aria-busy`, and a polite status that was in the
// document from the first render says "Loading…" and then "Loaded".
//
// # What to keep true when you change it
//
// * **Set `busy` to false when the content arrives,** or replace the skeleton
//   with the content. A skeleton left busy tells a reader the page is still
//   loading.
// * **Nothing real goes in a box.** A box is `aria-hidden`, so text inside one
//   is text nobody using a screen reader hears.
// * **Match the content's shape,** so the page does not jump when it arrives.
// * **It does not shimmer.** uf's StyleX has no `@keyframes` yet, and a still
//   shape is also the one a reader who asked for reduced motion wants.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import { Skeleton } from "@uniflowed/ui";

/** The shape a box stands in for. */
export type SkeletonShape = "line" | "block" | "circle";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  root: {
    display: "grid",
    gap: ufTokens.space3,
  },
  box: {
    display: "block",
    flexShrink: 0,
    backgroundColor: ufTokens.sunken,
    borderRadius: ufTokens.radiusSm,
  },
  line: {
    width: "100%",
    height: ufTokens.textMd,
  },
  block: {
    width: "100%",
    height: "5rem",
    borderRadius: ufTokens.radiusMd,
  },
  circle: {
    width: "40px",
    height: "40px",
    borderRadius: ufTokens.radiusPill,
  },
});

/** The region a skeleton stands in for. `busy` goes false once the content is there. */
component SkeletonRoot(
  children: React.Node,
  busy?: boolean = true,
  label?: string = "Loading…",
  doneLabel?: string = "Loaded",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Skeleton.Root
      {...forwarded(rest)}
      busy={busy}
      className={classNames(props(styles.root, xstyle).className, className)}
      doneLabel={doneLabel}
      label={label}
    >
      {children}
    </Skeleton.Root>
  );
}

/** One shape. Size it with `xstyle` when the content it stands in for is another size. */
component SkeletonBox(
  shape?: SkeletonShape = "line",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const styled = props(
    styles.box,
    match (shape) {
      "line" => styles.line,
      "block" => styles.block,
      "circle" => styles.circle,
    },
    xstyle,
  );
  return <Skeleton.Box {...forwarded(rest)} className={classNames(styled.className, className)} />;
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
 * The parts, under the names `import * as Skeleton from "./skeleton.js"` gives them.
 *
 * One name per component (ubugeeei-prod/uf#1453): a page writes `<Skeleton.Root>`
 * and `<Skeleton.Box>`, the way it writes `@uniflowed/ui`'s own parts. Each
 * is declared under its full name, so React DevTools and an error say
 * `SkeletonRoot` rather than `Root`.
 */
export { SkeletonRoot as Root, SkeletonBox as Box };
