// @flow
//
// Internal to `@uniflowed/router`: a server action as a form the browser can
// submit before any JavaScript has run.
//
// React 19's contract for `<form action={serverFunction}>` is progressive
// enhancement. While rendering to HTML, React asks the function for
// `$$FORM_ACTION(prefix)` and writes what it answers into the markup — a
// `method`, an `encType`, a hidden field named `name`, and one hidden field per
// entry of `data` — so the form is a real form that posts to the page it is on.
// A function without the property gets `action="javascript:throw …"` instead,
// which is what every uf form got before ubugeeei-prod/uf#1358.
//
// Two functions carry the property: the reference the browser holds
// (`createServerReference` in `../action.js`) and the real function the server
// renders with (`registerServerAction`, which `@uniflowed/vite` calls on every
// callable export of a `"use server"` module in the server graph). Both answer
// with the same fields, from the code below, so the markup the server writes and
// what a hydrated page would have written agree.
//
// # The fields
//
// React hands `$$FORM_ACTION` a prefix unique to the form in its render, and
// the prefix is in every field name so that two forms — or a `<form>` and a
// `<button formAction>` inside it — never read each other's fields:
//
// | Field | Value |
// | --- | --- |
// | `$uf_ref_<prefix>` | empty. The field that says *which* action this submit is for; on a `<button formAction>` it is the button's own name, so only the button that was pressed sends it |
// | `$uf_id_<prefix>` | the action's id, the same 64 hexadecimal characters the JSON call carries in `uf-action` |
// | `$uf_bound_<prefix>` | the bound arguments, as the JSON envelope `encodeActionArguments` writes, when there are any |
//
// `useActionState` adds React's own `$ACTION_KEY`, which names the hook that
// submitted, so the answer can put the action's result back into the same hook.
//
// Every field is a string and every bound argument goes through the same
// grammar as a JSON call: nothing new is allowed to cross by arriving as a form.
// `./action-endpoint.js` is the other half, and its header says what a native
// post is refused for.

import { encodeActionArguments } from "./action-wire.js";

/** The name of the field that says which action a submit is for. */
export const FORM_REF_FIELD = "$uf_ref_";
/** The action id, beside it. */
export const FORM_ID_FIELD = "$uf_id_";
/** The bound arguments, beside it. */
export const FORM_BOUND_FIELD = "$uf_bound_";
/** React's own: which `useActionState` submitted. */
export const FORM_STATE_KEY_FIELD = "$ACTION_KEY";

/**
 * The only content type a native action post may have.
 *
 * `application/x-www-form-urlencoded` rather than `multipart/form-data`,
 * because a server action refuses a file anyway and multipart parsing is a
 * surface uf does not open for one (`docs/security.md`). `$$FORM_ACTION` says
 * it explicitly, so React writes it on the form and on a submit button.
 */
export const FORM_ACTION_CONTENT_TYPE = "application/x-www-form-urlencoded";

/** What `$$FORM_ACTION` answers, in the shape React reads. */
export type FormActionFields = {|
  readonly name: string,
  readonly method: "POST",
  readonly encType: string,
  readonly data: FormData,
|};

/**
 * What a `useActionState` postback leaves for the page it renders.
 *
 * React's `ReactFormState`: the action's result, the hook's key, the action's
 * id, and how many bound arguments it had *besides* the state
 * `useActionState` bound itself.
 */
export type FormState = [mixed, string, string, number];

/**
 * Make `fn` a server action a form can post to without JavaScript.
 *
 * Defines three non-enumerable properties on `fn` and returns it:
 *
 * - `$$FORM_ACTION(prefix)`, the fields above;
 * - `$$IS_SIGNATURE_EQUAL(id, bound)`, which React asks while rendering a
 *   postback's answer to decide whether a `useActionState` is the one that
 *   submitted — the same action, bound the same number of times;
 * - `bind`, which keeps both of those on the function it returns, with the
 *   bound arguments recorded so they travel in the form.
 *
 * `call` is what the bound function runs: `fn` itself on the server, the
 * network call in the browser.
 */
export function withFormAction<T extends (...args: $ReadOnlyArray<empty>) => mixed>(
  fn: T,
  id: string,
  bound: $ReadOnlyArray<mixed>,
): T {
  const define = (name: string, value: mixed) => {
    Object.defineProperty(fn, name, { value, configurable: true, writable: true });
  };
  define("$$FORM_ACTION", (prefix: string): FormActionFields => formFields(id, bound, prefix));
  define(
    "$$IS_SIGNATURE_EQUAL",
    (referenceId: string, boundCount: number): boolean =>
      referenceId === id && boundCount === bound.length,
  );
  // `Function.prototype.bind` itself, reached through a cast: Flow types
  // `bind` per call site and has no type for the method taken off the
  // prototype and applied to an arbitrary function.
  const nativeBind: $FlowFixMe = Function.prototype.bind;
  define("bind", function bindAction(thisArg: mixed, ...more: Array<mixed>) {
    const next: T = nativeBind.call(fn, thisArg, ...more);
    return withFormAction(next, id, [...bound, ...more]);
  });
  return fn;
}

/**
 * The fields a form bound to this action is written with.
 *
 * Throws `ActionValueError` when a bound argument cannot cross, which React
 * catches and reports as "Failed to serialize an action for progressive
 * enhancement" before writing the form it would have written without this —
 * so an unencodable state costs the pre-hydration submit and nothing else.
 */
export function formFields(
  id: string,
  bound: $ReadOnlyArray<mixed>,
  prefix: string,
): FormActionFields {
  const data = new FormData();
  data.append(`${FORM_ID_FIELD}${prefix}`, id);
  if (bound.length > 0) {
    data.append(`${FORM_BOUND_FIELD}${prefix}`, encodeActionArguments(bound));
  }
  return {
    name: `${FORM_REF_FIELD}${prefix}`,
    method: "POST",
    encType: FORM_ACTION_CONTENT_TYPE,
    data,
  };
}

/** What a native post names, read off its fields. */
export type FormPost = {|
  /** The action id, as sent; checked by the caller. */
  readonly id: string | null,
  /** The bound-argument envelope, as sent, or `null` when there is none. */
  readonly bound: string | null,
  /** `$ACTION_KEY`, or `null` when no `useActionState` submitted. */
  readonly stateKey: string | null,
  /** Every other field, in order: what the action is handed as its form. */
  readonly entries: Array<[string, string]>,
|};

/**
 * Read a native post's fields, or `null` when it names no action at all.
 *
 * `null` is the caller's signal to decline, exactly as a JSON request with no
 * `uf-action` header is: an ordinary form posted to a route handler must reach
 * that handler. The last `$uf_ref_` field wins, because the browser writes a
 * pressed button's name after the fields of the form around it, and a button's
 * `formAction` is meant to override the form's `action`.
 *
 * Every field whose name uf or React owns is kept out of `entries`, including
 * another prefix's, so the action receives the form the person filled in and
 * nothing that says how it was wired.
 */
export function readFormPost(fields: URLSearchParams): FormPost | null {
  let prefix: string | null = null;
  for (const [name] of fields) {
    if (name.startsWith(FORM_REF_FIELD)) {
      prefix = name.slice(FORM_REF_FIELD.length);
    }
  }
  if (prefix == null) {
    return null;
  }
  const entries: Array<[string, string]> = [];
  for (const [name, value] of fields) {
    if (!isWiring(name)) {
      entries.push([name, value]);
    }
  }
  return {
    id: fields.get(`${FORM_ID_FIELD}${prefix}`),
    bound: fields.get(`${FORM_BOUND_FIELD}${prefix}`),
    stateKey: fields.get(FORM_STATE_KEY_FIELD),
    entries,
  };
}

/** Whether a field is uf's or React's rather than the form's own. */
function isWiring(name: string): boolean {
  return name.startsWith("$uf_") || name.startsWith("$ACTION_");
}

/** The element a postback's form state is written into the document as. */
export const FORM_STATE_ELEMENT_ID = "uf:form-state";

/**
 * The `<script>` that carries a postback's form state to `hydrateRoot`.
 *
 * `application/json`, so it is data and never executed — no nonce, and nothing
 * a Content Security Policy has to admit. The result was already held to the
 * wire grammar before it got here, so it is plain JSON; `<` is escaped so
 * that no string in it can close the element.
 */
export function formStateScript(state: FormState): string {
  const json = JSON.stringify(state).replace(/</g, "\\u003c");
  return `<script type="application/json" id="${FORM_STATE_ELEMENT_ID}">${json}</script>`;
}

/**
 * The form state a document carries, or `undefined`.
 *
 * Read by both hydrating entries before `hydrateRoot`. Anything malformed is
 * `undefined` rather than an exception: the page still hydrates, and the
 * `useActionState` that submitted starts from its initial state, which is what
 * it would have done with JavaScript from the start.
 */
export function readFormState(document: Document): FormState | void {
  const element = document.getElementById(FORM_STATE_ELEMENT_ID);
  if (element == null) {
    return undefined;
  }
  try {
    const parsed: mixed = JSON.parse(element.textContent);
    if (
      Array.isArray(parsed) &&
      parsed.length === 4 &&
      typeof parsed[1] === "string" &&
      typeof parsed[2] === "string" &&
      typeof parsed[3] === "number"
    ) {
      return [parsed[0], parsed[1], parsed[2], parsed[3]];
    }
  } catch {
    // Fall through: see above.
  }
  return undefined;
}
