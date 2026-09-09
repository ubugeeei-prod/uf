// @flow
//
// A callout, and the live region it must not be by default.
//
// Of the twenty components in the catalogue that look like a class list, this
// is the one whose usual shape is arguably wrong to copy rather than merely
// empty. Every version of it renders `<div role="alert">`, always, and that one
// attribute is a decision about interrupting the reader that nobody made.
//
// # `role="alert"` is a live region, not a colour
//
// A live region announces *changes*. An element carrying one that is already in
// the document when the page loads has no change to report, so it is announced
// on insertion or it is not announced at all — and which of those you get is a
// property of the moment the element entered the document, not of the element.
//
// So a permanently rendered "your trial ends soon" box with `role="alert"` is
// one of two things, both bad:
//
//   * an **interruption on every page load**, on the engines that treat the
//     initial render as an insertion — the reader is pulled out of whatever
//     they were doing to hear a sentence that was equally true yesterday;
//   * or **silence**, on the engines that do not — in which case the role was
//     decoration, and the box is read in its ordinary place in the page like
//     the `<div>` it is.
//
// Neither is what the author wanted, and neither is visible in a screenshot.
// The two cases have to be told apart by the caller, because the caller is the
// only one who knows which one they have:
//
//   * a **static callout** — a panel that is part of the page — is a container
//     with a heading and no live semantics at all. It is read where a reader
//     reaches it, and heading navigation finds it, which is what `Alert.Title`
//     being a real heading is for.
//   * an **alert** — something that appeared because something happened — is
//     `live`, and is `role="alert"`.
//
// `field.js` already makes exactly this call for `Field.Error`, which is
// rendered only once the field is wrong and is `role="alert"` for that reason.
//
// # Why `live` is a boolean and there is no polite option
//
// Because a polite one cannot be built this way, and offering it would be
// offering silence. `combobox.js` states the rule: a live region added to the
// page in the same commit as the text it holds is usually not announced,
// because the technology watching it had nothing to watch until it was already
// too late. `role="status"` is polite, so it is subject to that rule in full —
// a polite region has to have been in the document *first*, empty, and a
// component you render at the moment the thing happens never was.
//
// `role="alert"` is assertive, and assertive regions are announced on insertion
// by every engine that implements them; that is what the role is for. So the
// one live shape this component can honestly offer is the assertive one.
//
// The polite, page-level shape is `Toast`, which is the component that exists
// to have been watching already — `toast.js` and ubugeeei-prod/uf#289. An
// application that wants "saved" said politely wants a toast, not an alert, and
// pointing at it is a better answer than a `live="polite"` that does nothing.
//
// # No `"use client"`
//
// It holds no state, listens to nothing and manages no focus. Which of the two
// alerts this is arrived as a prop, and the heading level did too. It renders
// on a server.

import * as React from "@uniflowed/react";

import type { Rest } from "./internal/merge-props.js";

/**
 * A callout: a panel that is part of the page, or one that just appeared.
 *
 * `live` is the whole component. Without it there is no role, deliberately —
 * a box a reader reaches in reading order needs no announcement, and giving it
 * one costs an interruption on every page load or nothing at all. With it the
 * container is `role="alert"`, which is assertive and therefore the one live
 * shape that is announced when it is inserted with its text in it.
 *
 *     {error != null && (
 *       <Alert.Root live>
 *         <Alert.Title>Could not save</Alert.Title>
 *         <Alert.Description>{error}</Alert.Description>
 *       </Alert.Root>
 *     )}
 *
 * Rendered unconditionally with `live` on it, this is the mistake the module
 * header is about: the role is a promise about a change, and a box that was
 * always there has no change to report.
 */
export component AlertRoot(children: React.Node, live?: boolean = false, ...rest: Rest) {
  return (
    <div {...rest} role={live ? "alert" : undefined}>
      {children}
    </div>
  );
}

/**
 * The callout's heading.
 *
 * A real heading rather than a bold `<div>`, because a heading is how a screen
 * reader user finds a region of a page without reading it — and a callout
 * nobody can jump to is a callout that has to be walked into.
 *
 * `level` is the caller's for the reason `accordion.js` gives for the same
 * prop: the level that keeps a document outline true depends on what the
 * callout is inside, and a hard-coded one produces an outline nobody can
 * navigate. The guess is stated rather than hidden — `3`, which is right for a
 * callout inside a section that has a title of its own — and a level outside
 * the six HTML has is clamped, because `<h7>` is not an element and is
 * announced as nothing at all.
 */
export component AlertTitle(children: React.Node, level?: number = 3, ...rest: Rest) {
  const clamped = Math.min(6, Math.max(1, Math.trunc(level)));
  const Heading = `h${String(clamped)}`;

  return <Heading {...rest}>{children}</Heading>;
}

/** What the callout says, under its heading. */
export component AlertDescription(children: React.Node, ...rest: Rest) {
  return <p {...rest}>{children}</p>;
}
