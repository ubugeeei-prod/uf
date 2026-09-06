// @flow
//
// What a `<form>` submits for a control the browser has never heard of.
//
// Every widget in this package is a `button` or a `div` wearing an ARIA role,
// which is what makes it styleable and what makes it invisible to form
// submission: `new FormData(form)` collects the form's *listed* elements, and a
// `<div role="listbox">` is not one. So a Select inside a form submitted
// nothing at all, and a Combobox submitted `Combobox.Input`'s value — which is
// the label the reader sees and not the value the application meant. A country
// picker posted "United Kingdom" where the server was waiting for `GB`.
//
// The fix is one hidden `<input>` carrying the real value, rendered only when
// the caller asked for one by giving a `name`. No `name`, no control: a Select
// used to drive a filter has nothing to submit, and a form field the caller
// never named is not one this package should invent.
//
// # Why a hidden input and not a hidden `<select>`
//
// Rendering a real, visually hidden `<select>` is the other answer, and it buys
// two things: the browser autofills it, and `required` gets native constraint
// validation. Both were tried and neither survives contact with the
// accessibility tree.
//
// A `<select>` is focusable, so it is announced. A reader who tabs into the
// widget hears the styled combobox and then a second, invisible combobox with
// the same options — the duplicate-announcement bug that makes people describe
// a component library as "noisy". Taking it out of the tree means
// `aria-hidden="true"`, and `aria-hidden` on a focusable element is itself the
// violation: it hides the element from a screen reader while leaving it in the
// tab order, so the reader lands on something their software says is not there.
// `tabindex="-1"` plus `aria-hidden` closes that hole and gives up the tab
// order, which is the autofill affordance the native control was for.
//
// And native validation cannot work here either. The browser reports a
// constraint failure by focusing the invalid control and drawing a bubble at
// it; on a control with no box, Chrome logs "An invalid form control with
// name='country' is not focusable" and refuses to submit the form at all, with
// nothing shown to the reader. A headless select's `required` therefore belongs
// to `@uniflowed/form` and `@uniflowed/validator`, which is where uf already
// put every other rule, and `Field.Error` is where the message goes.
//
// An `<input type="hidden">` is none of those things: never focusable, never in
// the accessibility tree, never validated, and always submitted. What it costs
// is autofill, which is a real loss and is written down rather than hidden —
// a browser will not fill a hidden input the way it fills `<select
// name="country">`.
//
// # This is not how `@uniflowed/form` reads a value
//
// Worth stating because the two look like they overlap and do not.
// `@uniflowed/form` holds values in its own store and calls `preventDefault()`
// on submit, so it never builds a `FormData` and never sees this element. A
// Select bound to that library is bound through `useController` — `field.value`
// into `value`, `field.onChange` into `onValueChange` — and needs no `name`
// here at all. This element is for the other kind of form: a plain `<form
// action={…}>`, a Server Action, or anything else that submits the document.
//
// # Why this is `internal/` and not a subpath
//
// It is one sentence about what this package promises a form, and the failure
// mode of writing it twice is that Select and Combobox disagree about what a
// disabled control submits. Exported, it would be a `<HiddenInput>` a consumer
// could reach for in a component that had not thought about any of the above.

"use client";

/**
 * The control a form actually reads.
 *
 * `value` is the widget's value, not its label. `null` renders an empty string
 * rather than omitting the control, so a form that submits a Select the reader
 * left alone still carries the field — a key missing from the payload and a key
 * present and empty are different questions to a server, and "the reader saw
 * this field and chose nothing" is the second one.
 *
 * `disabled` is passed through rather than interpreted: a disabled control is
 * omitted from the submission by the browser, which is the behaviour a native
 * `<select disabled>` has and the one a caller who disabled the widget expects.
 */
export component FormValue(name: string, value: string | null, disabled?: boolean = false) {
  return <input disabled={disabled} name={name} type="hidden" value={value ?? ""} />;
}
