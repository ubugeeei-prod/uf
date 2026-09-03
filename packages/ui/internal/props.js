// @flow
//
// Merging a caller's props with the ones a component owns.
//
// `<div {...rest} role="dialog">` and `<div role="dialog" {...rest}>` are
// different components. The second lets a caller pass `role="button"` and get
// it; the first does not. That sounds like a preference until you notice what
// else is in `rest`:
//
//   * A caller `ref` replaced the ref the dialog uses to find its focus stops,
//     so `contentRef.current` stayed null, the Tab handler returned early, and
//     the focus trap was *silently off* while the dialog still announced
//     `aria-modal="true"`.
//   * A caller `onClick` replaced a tab's selection handler, so clicking a tab
//     did nothing.
//   * A caller `onKeyDown` replaced the dialog's, so Escape stopped closing it.
//
// None of those fail loudly. So the rule here is: the caller's props go on
// first and the component's own semantics go on last, and for the two kinds of
// prop where a caller legitimately wants *both* — event handlers and refs —
// they are composed rather than one replacing the other.

/** Anything a caller can spread onto an element. */
export type Rest = { +[string]: mixed };

/**
 * Call the caller's handler and then the component's.
 *
 * The caller's runs first so it can inspect the event before the component
 * acts on it, and the component's runs unless the caller stopped the event —
 * `defaultPrevented` is the caller's way of saying "I handled this", which is
 * the same contract the DOM uses.
 */
export function composeHandlers<TEvent: { +defaultPrevented?: boolean }>(
  theirs: mixed,
  ours: (event: TEvent) => mixed,
): (event: TEvent) => mixed {
  if (typeof theirs !== "function") {
    return ours;
  }
  return (event: TEvent) => {
    (theirs: $FlowFixMe)(event);
    if (event.defaultPrevented !== true) {
      ours(event);
    }
  };
}

/** Set both refs, whichever kinds they are. */
export function composeRefs<T>(
  theirs: mixed,
  ours: (value: T | null) => mixed,
): (value: T | null) => void {
  return (value: T | null) => {
    ours(value);
    if (typeof theirs === "function") {
      (theirs: $FlowFixMe)(value);
    } else if (theirs != null && typeof theirs === "object") {
      (theirs: $FlowFixMe).current = value;
    }
  };
}

/**
 * A caller's props with the handlers and ref removed.
 *
 * They are pulled out because they have to be composed rather than spread, and
 * leaving them in would put the caller's copy back on top of the composed one.
 */
export function withoutComposed(rest: Rest, names: $ReadOnlyArray<string>): Rest {
  const kept: { [string]: mixed } = {};
  for (const key of Object.keys(rest)) {
    if (!names.includes(key)) {
      kept[key] = rest[key];
    }
  }
  return kept;
}
