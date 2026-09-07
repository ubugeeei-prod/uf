// @flow
//
// Ways of firing an event that have to be type errors, and the test that says
// they are.
//
// The companion to `matchers.js`, for the other surface a proxy used to hide.
// `@uniflowed/react-testing` published `fireEvent` as `any`, so
// `fireEvent.clcik(button)` was a call that dispatched a `clcik` event, which
// no component listens for, in a test that then asserted nothing had happened
// and passed. Read the same way as every fixture here: this file is *supposed*
// to fail `uf check`, a `// expect:` comment says the line after it must be
// reported and what the report must contain, and a line without one must not be
// reported at all.
//
// The last two entries are the cost of the table rather than a bug in it. The
// proxy answered to any name in any casing; a written-out list answers to a
// hundred-odd names in one casing, and `fireEvent(target, name, init)` — which
// is checked below and is not an error — is where a computed or unlisted name
// goes. Both are marked here so that the loss is a thing somebody decided
// rather than a thing somebody discovers.

import { fireEvent } from "../../packages/react-testing/index.js";

const button: HTMLElement = document.createElement("button");

// A misspelt event name. The whole reason the table exists.
// expect: clcik
export const misspelt: boolean = fireEvent.clcik(button);

// A name nobody listed. The proxy dispatched it; the table says so.
// expect: pointerDoubleTap
export const unlisted: boolean = fireEvent.pointerDoubleTap(button);

// The DOM's own lower-case spelling. The proxy took it because it lower-cased
// whatever it was handed, and the table publishes the camel case React and
// Testing Library publish.
// expect: keydown
export const lowerCased: boolean = fireEvent.keydown(button);

// A target is an `EventTarget`, and a string is not one.
// expect: EventTarget
export const notATarget: boolean = fireEvent.click("#save");

// Everything below is correct, and has to stay unreported.
export const clicked: boolean = fireEvent.click(button);
export const keyed: boolean = fireEvent.keyDown(button, { key: "Escape" });
export const pointed: boolean = fireEvent.pointerEnter(button);
export const submitted: boolean = fireEvent.submit(button);
export const computed: boolean = fireEvent(button, "pointerdoubletap");
