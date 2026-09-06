// @flow
//
// What a reader can reach, in the order they reach it.
//
// One list, asked for by two components that want opposite things from it.
// `Dialog.Body` uses it to keep focus *in*: the first and last entries are
// where `Tab` and `Shift+Tab` wrap. `Popover.Body` uses it to put focus in
// once and then leaves it alone, because tabbing out of a popover is how a
// reader leaves one. The list has to be the same list for those two to be
// describable as different policies over the same fact rather than as two
// components that disagree about what focusable means.
//
// # The selector is the browser's rule, written down
//
// A disabled control is out because the browser will not focus one, and
// `tabindex="-1"` is out because it means "focusable by script, not by Tab" —
// which is what every roving tab stop in this package uses, so a menu inside a
// dialog would otherwise report thirty items as focus stops and the trap would
// wrap between two of them instead of at the dialog's edges.
//
// The three ancestor checks are the ones a selector cannot make. `hidden`,
// `inert` and `aria-hidden="true"` each hide a whole subtree, and reading them
// off the element alone returned a button inside `<div aria-hidden="true">` as
// a focus stop — after which the trap moved focus to a control no screen
// reader exposes and the reader was somewhere they could not be told about.
//
// # Why this is `internal/` and not a subpath
//
// The same reason `merge-props.js` gives. A public `focusable()` is a
// general-purpose DOM utility, and a second, weaker copy of one is how two
// parts of a package come to disagree about which elements exist. What is
// shipped here is narrower: the definition this package's focus behaviour is
// written against.

/**
 * The elements a browser will move focus to with `Tab`.
 *
 * Exported as the selector as well as through `focusable`, because one
 * question is asked about a single element rather than about a subtree: a
 * tooltip's trigger has to *be* one of these or the tooltip is one only a mouse
 * can reach, and `element.matches(FOCUS_STOPS)` is that question.
 */
export const FOCUS_STOPS: string =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * The focus stops inside an element, in document order.
 *
 * Document order rather than mount order, for the reason
 * `internal/roving-focus.js` gives about items: the two stop agreeing the first
 * time something is rendered conditionally, and the reader's `Tab` follows the
 * document.
 */
export function focusable(root: HTMLElement): Array<HTMLElement> {
  return Array.from(root.querySelectorAll(FOCUS_STOPS)).filter(
    (element: $FlowFixMe) =>
      // All three hide a whole subtree, so all three are asked of the
      // ancestors; see the module header for what reading them off the element
      // alone let through.
      element.closest("[hidden]") == null &&
      element.closest("[inert]") == null &&
      element.closest('[aria-hidden="true"]') == null,
  );
}
