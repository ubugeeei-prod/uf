// @flow
//
// The DOM half of an uncontrolled field: reading a value out of a control, and
// putting one back.
//
// This is the module that makes "uncontrolled by default" possible, and it is
// deliberately the only one that knows what an `<input>` is. Everything above
// it — the store, `register`, validation, `useFieldArray` — works in terms of
// field paths and plain values, so the same store drives a controlled field
// through `useController` with no DOM in sight, and a server render that never
// mounts a control still has every value.
//
// # Why this is not `element.value`
//
// Because a form is not made of text boxes. The same field path can be a
// checkbox whose value is a boolean, a group of checkboxes whose value is an
// array, a radio group where the value belongs to whichever member is checked,
// a `<select multiple>` whose value is the selected options, or a file input
// whose value is a `FileList` that cannot be assigned at all. A library that
// reads `element.value` gets `"on"` from a checkbox and `""` from a radio
// group, and the bug shows up as a form that submits the wrong shape rather
// than as an error.
//
// # Why a field holds a list of elements
//
// A radio group is one field and several DOM nodes with the same `name`, and so
// is a checkbox group. `register("colour")` is called once per input, and each
// call attaches another element to the same field record. Reading the field
// then means asking the group, not the node — which is why [`readElements`]
// takes the list.
//
// Detached nodes are skipped rather than removed here: React calls a ref's
// cleanup for the node that is going away and the ref for the node arriving,
// but a re-ordered field array can interleave them, and a group that dropped
// its only remaining member mid-commit would read as empty for one event.

import type { FieldValues } from "./field-path.js";

/** How a raw DOM value is turned into the value the form keeps. */
export type ValueTransform = {|
  /** Parse the control's text as a number, `NaN` when it is not one. */
  readonly valueAsNumber?: boolean,
  /** Parse the control's text as a `Date`, `null` when it is not one. */
  readonly valueAsDate?: boolean,
  /** Anything else: run last, and given whatever the steps above produced. */
  readonly setValueAs?: (value: mixed) => mixed,
|};

type AnyElement = $FlowFixMe;

function typeOf(element: AnyElement): string {
  return String(element?.type ?? "").toLowerCase();
}

function tagOf(element: AnyElement): string {
  return String(element?.tagName ?? "").toLowerCase();
}

/** Whether the node is still in a document, and so still part of its group. */
function isLive(element: AnyElement): boolean {
  return element != null && (element.isConnected !== false || element.ownerDocument == null);
}

/**
 * The value of the field these elements make up.
 *
 * The shape depends on what the elements are, and each case is the shape a
 * person would write by hand if the field were the only one in the form:
 *
 * - one checkbox — a boolean, or its `value` attribute when it has a
 *   meaningful one, because `<input type="checkbox" value="yes">` is how a
 *   single-choice checkbox is written and `"on"` is what the DOM invents when
 *   it is not;
 * - several checkboxes — the values of the checked ones, as an array;
 * - a radio group — the value of the checked member, or `null` for none;
 * - `<select multiple>` — the selected values, as an array;
 * - a file input — the `FileList`, untouched;
 * - anything else — `element.value`, a string.
 */
export function readElements(
  elements: $ReadOnlyArray<AnyElement>,
  transform?: ValueTransform,
): mixed {
  const live = elements.filter(isLive);
  if (live.length === 0) {
    return undefined;
  }

  const first = live[0];
  const kind = typeOf(first);

  if (kind === "radio") {
    const checked = live.find((element) => element.checked === true);
    return checked == null ? null : applyTransform(checked.value, transform);
  }

  if (kind === "checkbox") {
    if (live.length > 1) {
      return live
        .filter((element) => element.checked === true)
        .map((element) => applyTransform(element.value, transform));
    }
    const named = String(first.value ?? "");
    if (named !== "" && named !== "on") {
      // `<input type="checkbox" value="marketing">` is a field whose value is
      // `"marketing"` when it is on and nothing when it is off. Reporting
      // `true` there loses the only information the control carried.
      return first.checked === true ? applyTransform(named, transform) : undefined;
    }
    return first.checked === true;
  }

  if (kind === "file") {
    return first.files;
  }

  if (tagOf(first) === "select" && first.multiple === true) {
    return Array.from(first.options ?? [])
      .filter((option: AnyElement) => option.selected === true)
      .map((option: AnyElement) => applyTransform(option.value, transform));
  }

  return applyTransform(first.value, transform);
}

/**
 * Apply the caller's conversions, in the order the options are documented.
 *
 * `valueAsNumber` on an empty control yields `NaN` rather than `0`: an empty
 * number input has no number in it, and `0` is a value the user could have
 * typed. A `required` rule then reports the field as missing, which is what a
 * reader of the form would say about it.
 */
function applyTransform(raw: mixed, transform?: ValueTransform): mixed {
  let value = raw;
  if (transform?.valueAsNumber === true) {
    value = value === "" || value == null ? Number.NaN : Number(value);
  } else if (transform?.valueAsDate === true) {
    const parsed = value === "" || value == null ? Number.NaN : new Date(String(value)).getTime();
    value = Number.isNaN(parsed) ? null : new Date(parsed);
  }
  return transform?.setValueAs == null ? value : transform.setValueAs(value);
}

/**
 * Put `value` back into the controls, which is what makes `reset` work.
 *
 * An uncontrolled input's displayed text belongs to the DOM, so a store that
 * changed its own copy and told React about it would leave the user looking at
 * the old text. `reset` and `setValue` write here for that reason, and this is
 * the whole of the "uncontrolled" contract: the store owns the value, the DOM
 * owns the display, and this function is the one place they are reconciled.
 *
 * A file input is skipped: its `value` is not assignable for a reason, and
 * pretending otherwise raises in every browser.
 */
export function writeElements(elements: $ReadOnlyArray<AnyElement>, value: mixed): void {
  const live = elements.filter(isLive);
  if (live.length === 0) {
    return;
  }

  const first = live[0];
  const kind = typeOf(first);

  if (kind === "radio") {
    for (const element of live) {
      element.checked = String(element.value) === String(value ?? "");
    }
    return;
  }

  if (kind === "checkbox") {
    if (live.length > 1 || Array.isArray(value)) {
      const chosen = Array.isArray(value) ? value.map(String) : [];
      for (const element of live) {
        element.checked = chosen.includes(String(element.value));
      }
      return;
    }
    const named = String(first.value ?? "");
    first.checked = named !== "" && named !== "on" ? String(value ?? "") === named : Boolean(value);
    return;
  }

  if (kind === "file") {
    return;
  }

  if (tagOf(first) === "select" && first.multiple === true) {
    const chosen = (Array.isArray(value) ? value : []).map(String);
    for (const option of Array.from(first.options ?? [])) {
      (option as AnyElement).selected = chosen.includes(String((option as AnyElement).value));
    }
    return;
  }

  first.value = value == null ? "" : String(value);
}

/**
 * Move focus to the field, and put the caret where a person would expect it.
 *
 * Called after a failed submit and by `setFocus`. `select()` on a text control
 * means the first thing the user types replaces the value that was rejected,
 * rather than being appended to it — which is nearly always what somebody
 * fixing a validation error is about to do.
 */
export function focusElement(element: AnyElement, select: boolean = false): void {
  if (element == null || typeof element.focus !== "function") {
    return;
  }
  element.focus();
  if (select && typeof element.select === "function") {
    element.select();
  }
}
