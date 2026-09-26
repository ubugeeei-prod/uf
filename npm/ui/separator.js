// @flow
//
// A rule, and the one decision in it: whether anybody is told it is there.
//
// Two lines of markup, and it belongs in this package rather than in the preset
// for the same reason `Progress` does — the component *is* a conditional about
// what a reader hears:
//
//   * A separator **between groups of content** is `role="separator"` with an
//     `aria-orientation`. A reader moving down the page is told the subject
//     changed, which is the information the line was drawn to give and the only
//     way they get it.
//   * A **decorative** rule — the line under a heading, the hairline between a
//     card's padding and its footer — is `aria-hidden="true"` and announced to
//     nobody. It is a border that happens to be an element.
//
// Getting it backwards is silent in both directions: a decorative rule with the
// role adds a "separator" to every reading of the page, and a real boundary
// without it takes the boundary away from everyone who is not looking at it.
//
// The default is the semantic one, because the two mistakes do not cost the
// same. A rule wrongly announced is noise a reader can hear and skip; a
// boundary wrongly silent is information that is simply not there, and nobody
// finds out. `progress.js` makes the same trade in its own sentence: the safe
// answer has to be the honest one.
//
// # Why this is a `<div>` and not an `<hr>`
//
// An `<hr>` already carries `role="separator"`, so for a horizontal rule
// between two blocks of prose it is the better answer and a caller who can use
// one should. This exists for what it cannot do.
//
// It comes with a border and a margin from the browser's own stylesheet, and a
// package that ships no styles cannot ship a visible line — every consumer
// would begin by turning it off. It is horizontal by definition, so a vertical
// rule between two things in a row is a rotated element rather than a described
// one. And it is a paragraph-level break in the flow, which is not what a
// hairline inside a toolbar is.
//
// # The two separators that are not this one
//
// `Menu.Separator` is the rule between groups of menu items, and belongs to the
// menu because it has to be skipped by the arrow keys that walk it.
// `Resizable.Handle` announces itself as a separator too, and is a *control* —
// the APG window splitter, a separator that behaves like a slider. Neither is
// this, and reaching for this one in either place loses the behaviour that made
// them their own components.
//
// # No `"use client"`
//
// One element, two attributes, nothing to remember. It renders on a server.

import type { Orientation } from "./internal/roving-focus.js";
import type { RenderProp, Rest } from "./internal/merge-props.js";
import { withProps } from "./internal/merge-props.js";

export type { Orientation } from "./internal/roving-focus.js";

/**
 * A rule between two things, or a line that is only a line.
 *
 * `decorative` is the whole component. Without it the element is a
 * `role="separator"` a reader is told about; with it the element is hidden from
 * the accessibility tree entirely.
 *
 *     <Separator />
 *     <Separator orientation="vertical" />
 *     <Separator decorative />
 *
 * The decorative one gets `aria-hidden` and no role, rather than
 * `role="presentation"` as well: a `<div>` has nothing to hide behind a
 * presentation role, and one attribute that removes the element from the tree
 * says the whole thing. `Breadcrumb.Separator` carries both because it is an
 * `<li>`, whose `listitem` role would otherwise be counted.
 *
 * `aria-orientation` is written out even for the horizontal case, where ARIA
 * would default to it. It is the attribute a reader of this markup is looking
 * for, and a default that is left implicit is a default somebody has to know.
 *
 * `render` changes the element carrying that decision, not the decision
 * itself: decorative rules stay hidden, semantic rules keep the separator
 * role and orientation.
 */
export component Separator(
  decorative?: boolean = false,
  orientation?: Orientation = "horizontal",
  render?: RenderProp,
  ...rest: Rest
) {
  const props = decorative
    ? withProps(rest, { "aria-hidden": "true" })
    : withProps(rest, { "aria-orientation": orientation, role: "separator" });
  if (render != null) {
    return render(props);
  }
  return <div {...props} />;
}
