// @flow
//
// Internal to `@uniflowed/router`: what both hydrating entries hand
// `hydrateRoot` besides the tree.

import type { FormState } from "./form-action.js";

/** React's `onRecoverableError`, as the development report builds one. */
type Recovery = (error: mixed, info: { componentStack?: ?string, ... }) => void;

/** What this hands `hydrateRoot`. */
type HydrationOptions = {| onRecoverableError?: Recovery, formState?: FormState |};

/**
 * `hydrateRoot`'s options: the development recovery handler, and a postback's
 * form state.
 *
 * `formState` is React's own option. A page rendered in answer to a form posted
 * before hydration was rendered with it, React marked the `useActionState` that
 * submitted, and passing the same value here is how the browser's copy of that
 * hook starts from the action's result instead of its initial state.
 */
export function hydrationOptions(
  recovery: ?Recovery,
  formState: FormState | void,
): HydrationOptions | void {
  if (recovery == null && formState == null) {
    return undefined;
  }
  const options: HydrationOptions = {};
  if (recovery != null) {
    options.onRecoverableError = recovery;
  }
  if (formState != null) {
    options.formState = formState;
  }
  return options;
}
