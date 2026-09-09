// @flow
//
// Internal to `@uniflowed/router`: what a server action's arguments and its
// result are allowed to be.
//
// A `"use server"` export is a public HTTP endpoint from the moment it exists.
// The one decision that makes it a feature rather than a remote-code-execution
// surface is what the bytes on the wire are permitted to become, and this
// module is that decision, written once and applied on both sides of it.
//
// # The boundary
//
// **A server action's arguments are plain JSON data, plus at most one form.**
//
// One JSON object, `{"args": [...]}`, at most `MAX_ACTION_BODY_BYTES` of valid
// UTF-8, holding at most `MAX_ACTION_ARGUMENTS` values, nested at most
// `MAX_ACTION_DEPTH` deep, with at most `MAX_ACTION_VALUES` values in total.
// Each of those values is `null`, a boolean, a finite number, a string, an
// array of them, or a plain object whose keys are ordinary strings. The result
// travels back under exactly the same grammar, plus `undefined` for an action
// that returns nothing — and with no form, because a form is something a
// browser submits and not something a server answers with.
//
// Nothing in a payload can name a function, a module, a class, a prototype, a
// React element, an id or a reference, and nothing in it is revived into an
// object the sender chose. `decodeActionArguments` produces values a
// `JSON.parse` already produced; what this adds is the refusal of everything
// `JSON.parse` would have let through — which is the whole of what a decoder
// has to get right.
//
// # The one thing that is not a value: `<form action={fn}>`
//
// React 19 hands a form action a `FormData`, and `useActionState` hands it the
// previous state and then a `FormData`. So one argument of a call may be a
// form, and the envelope grows a second key to say which:
//
//   {"args": [null, null], "form": {"at": 1, "entries": [["note", "hi"]]}}
//
// It is written *beside* the values rather than inside one, and that placement
// is the whole design. A tag in the value tree — `{"$formData": …}` — would be
// a payload saying which constructor to call, which is the shape of every
// deserialisation CVE and the thing the list below refuses on principle.
// Outside the tree there is no tag: `checkActionValue` is unchanged and still
// knows nothing but JSON data, `at` names a position and not a type, and the
// slot it names must hold `null` so a sender cannot say two things about one
// argument. At most one form crosses per call, it is never nested inside a
// value, its entries are `[name, value]` pairs of strings, and the decoder's
// only constructor is `FormData` — fixed here, never named by the payload.
//
// What that buys is `<form action={fn}>`, `useActionState` and `useFormStatus`
// against a real endpoint. What it does not buy is a form that works before
// hydration: React's progressive enhancement needs `$$FORM_ACTION` on the
// reference, which makes the submit a *native* form post, and a native form
// post is `multipart/form-data` — a content type this endpoint refuses on
// purpose (see `./action-endpoint.js`, rule 4). Without it React writes the
// form it writes for any client action, whose `action` is a `javascript:` URL
// that throws, so such a submit does nothing rather than posting somewhere it
// should not. That is still open; see ubugeeei-prod/uf#252.
//
// # What is deliberately absent, and why
//
// * **A reference format.** React's Flight payload can carry a reference to a
//   client module, a promise, or an element, and a decoder that reconstructs
//   those is a decoder that constructs attacker-chosen objects. uf's payload
//   (`./payload.js`) now carries one of the three — a reference to a *row of
//   itself*, which names nothing to construct — and it is a document the
//   server writes rather than a body somebody sends. This grammar is the one
//   an untrusted sender is decoded under, so it still has none, and it is
//   still not the place to grow one quietly (ubugeeei-prod/uf#252).
// * **Class instances, `Map`, `Set`, `Date`, `RegExp`, typed arrays.** Each
//   would need a tag in the payload saying which constructor to call, and a
//   tag naming a constructor is the oracle every deserialisation CVE is made
//   of. An action that wants a date takes an ISO string and parses it, where
//   the parse is the application's and is checked.
// * **A file in a form.** A `File` entry is refused at the call site rather
//   than encoded: bytes in this envelope would be base64 in a JSON string with
//   no ceiling of their own, and an upload wants a content type, a streaming
//   read and a size limit that are not this module's. Multipart parsing is its
//   own attack surface and is still deliberately absent.
// * **Cycles and shared references.** A value that refers to itself is
//   rejected rather than encoded, because the alternative is a marker in the
//   payload that says "this is the object you saw earlier", which is a
//   reference format by another name.
//
// # Why the same module runs on both sides
//
// The browser refuses to *send* what the server would refuse to receive, so a
// value that cannot cross is a mistake at the call site with the argument's
// position in the message, rather than a 400 with nothing in it. Two
// implementations of one grammar is how the two come to disagree, and the
// side that is lenient is always the server.
//
// Pure: no imports, no platform APIs beyond `JSON`, so the browser half of
// `@uniflowed/router` can reach it without reaching anything server-only.

/** The request header carrying the action id. */
export const ACTION_HEADER: string = "uf-action";

/**
 * The only content type an action call may be sent with.
 *
 * Not decoration. `application/json` is not one of the three types a form can
 * produce, so a cross-origin `<form>` — which is sent with the visitor's
 * cookies and no preflight — cannot reach the decoder at all. It is the second
 * of the three independent things standing between this endpoint and a CSRF,
 * the others being the `Origin` check and `ACTION_HEADER` itself, which is a
 * header no simple request may carry.
 */
export const ACTION_CONTENT_TYPE: string = "application/json";

/** Largest request body the endpoint will read, in bytes. */
export const MAX_ACTION_BODY_BYTES: number = 1024 * 1024;

/** Most positional arguments an action may be called with. */
export const MAX_ACTION_ARGUMENTS: number = 16;

/** Deepest nesting a payload may have. */
export const MAX_ACTION_DEPTH: number = 24;

/** Most values, of any kind, one payload may hold. */
export const MAX_ACTION_VALUES: number = 10000;

/**
 * Most fields one submitted form may carry.
 *
 * A ceiling on the *count* rather than on the bytes, because the bytes already
 * have one: the whole body is bounded by `MAX_ACTION_BODY_BYTES` before a
 * character of it is parsed. What this bounds is the number of `append` calls
 * a sender can make the decoder do, and the size of the multimap they build.
 * A form with more than 256 controls is a form that wants a different shape.
 */
export const MAX_FORM_ENTRIES: number = 256;

/**
 * Longest field name one form entry may have.
 *
 * A name is an HTML `name` attribute — `email`, `items[3][quantity]` — so this
 * is generous by two orders of magnitude for anything a document declares, and
 * it stops a body's whole byte budget being spent on one key.
 */
export const MAX_FORM_NAME_LENGTH: number = 128;

/**
 * Everything that may cross the wire.
 *
 * Recursive on purpose, and closed on purpose: this type is what
 * `ServerActionBoundary` in `../action.js` holds every action's parameters and
 * return value against, so an action that takes a `Map` is a `uf check` error
 * rather than a request that arrives with an empty object in it.
 */
export type ActionValue =
  | null
  | boolean
  | number
  | string
  | $ReadOnlyArray<ActionValue>
  | { readonly [string]: ActionValue };

/**
 * Everything an *argument* may be: a value, or the one form.
 *
 * Wider than [`ActionValue`] in exactly one place and deliberately not
 * recursive: a `FormData` is something a call passes, never something inside
 * something a call passes. `ActionArguments` in `../action.js` holds every
 * action's parameter list against this, and `ActionResult` still holds every
 * return type against `ActionValue` — an action receives a form and does not
 * answer with one.
 */
export type ActionArgument = ActionValue | FormData;

/**
 * A value that is outside the grammar, and where in the payload it was.
 *
 * Thrown on the browser side, where the path is the argument the caller
 * passed. On the server side it is caught and becomes a `400` with none of
 * this in it: the sender does not get told which part of what they sent was
 * the part that was refused.
 */
export class ActionValueError extends Error {
  /** Where in the payload the offending value sat, e.g. `argument 2.name`. */
  path: string;

  constructor(path: string, reason: string) {
    super(`@uniflowed/router: ${path} ${reason}.`);
    this.name = "ActionValueError";
    this.path = path;
  }
}

/**
 * Keys that are never a property in this grammar.
 *
 * `JSON.parse` gives `__proto__` an *own* data property rather than changing
 * the prototype, so a payload carrying one is not itself pollution — it
 * becomes pollution in the first line of application code that spreads or
 * merges it. Refused here, where there is one place to refuse it, rather than
 * left for every action to remember. `uf_pm::detect` draws the same line for
 * manifest JSON.
 */
const FORBIDDEN_KEYS: $ReadOnlyArray<string> = ["__proto__", "constructor", "prototype"];

/**
 * Whether an object is one this grammar carries.
 *
 * The prototype is compared, not `instanceof` and not a duck-type: a class
 * instance, a `Map`, a `Date` and a React element all pass every structural
 * test somebody might reach for, and each of them loses its meaning on the
 * wire. `Object.create(null)` is accepted because it is what a careful caller
 * builds and what this module's own decoder could produce.
 */
function isPlainObject(value: interface {}): boolean {
  const prototype: mixed = Object.getPrototypeOf(value);
  return prototype === PLAIN_PROTOTYPE || prototype === null;
}

/**
 * The prototype every object literal has, captured rather than named.
 *
 * `Object.prototype` is not on Flow's `Object` statics, and the usual
 * substitute — asking whether the prototype's own prototype is `null` — also
 * accepts `Object.create(Object.create(null))`, which is a different set of
 * values. One `{}` at module scope is the exact answer and costs nothing.
 */
const PLAIN_PROTOTYPE: mixed = Object.getPrototypeOf({});

/**
 * The value as a submitted form, or `null`.
 *
 * `typeof` first, because this module is imported by the browser's half and by
 * the server's, and `FormData` is a global that a runtime is allowed not to
 * have. `instanceof` and not a duck-type, for the reason `isPlainObject` gives
 * about prototypes: an object with `entries` and `get` is not a form.
 *
 * It answers with the form rather than with a `boolean` so that the caller
 * that goes on to read the entries has the type from the check rather than
 * from a cast. A Flow type guard would be the direct spelling and is not
 * available here: a guard has to refine the type away on the false branch too,
 * and "this runtime has no `FormData` at all" is a false branch that says
 * nothing about the value.
 */
function asFormData(value: mixed): FormData | null {
  if (typeof FormData === "undefined" || !(value instanceof FormData)) {
    return null;
  }
  return value;
}

/** One entry of the explicit walk stack. */
type Pending = {| readonly value: mixed, readonly path: string, readonly depth: number |};

/**
 * Refuse a value that cannot cross the wire, naming where it was.
 *
 * An explicit stack rather than recursion, for the reason the RSC graph walk
 * gives: a payload arrives from the network, its nesting is the sender's
 * choice, and a recursive walk over it is a stack overflow with an attacker's
 * hand on the depth. Every bound is checked as the walk runs rather than
 * afterwards, so a payload that busts one is refused before the rest of it is
 * visited.
 */
export function checkActionValue(root: mixed, label: string): void {
  const stack: Array<Pending> = [{ value: root, path: label, depth: 0 }];
  const seen = new Set<mixed>();
  let remaining = MAX_ACTION_VALUES;

  while (stack.length > 0) {
    const pending = stack.pop();
    if (pending == null) {
      break;
    }
    const { value, path, depth } = pending;

    remaining -= 1;
    if (remaining < 0) {
      throw new ActionValueError(label, `holds more than ${String(MAX_ACTION_VALUES)} values`);
    }
    if (depth > MAX_ACTION_DEPTH) {
      throw new ActionValueError(path, `is nested deeper than ${String(MAX_ACTION_DEPTH)}`);
    }

    if (value === null) {
      continue;
    }
    const kind = typeof value;
    if (kind === "boolean" || kind === "string") {
      continue;
    }
    if (kind === "number") {
      // `NaN` and the infinities have no JSON spelling; `JSON.stringify` writes
      // `null` for each, so accepting one here would mean an action was called
      // with a different number from the one the caller passed.
      if (!Number.isFinite(value)) {
        throw new ActionValueError(path, "is not a finite number");
      }
      continue;
    }
    if (kind !== "object") {
      // `undefined`, a function, a symbol, a bigint. Named rather than
      // lumped together, because "a function cannot cross" is the sentence
      // that tells a caller they passed a callback to a server action.
      throw new ActionValueError(path, `is a ${kind}, which cannot cross to a server action`);
    }

    const object: interface {} = value as $FlowFixMe;
    if (seen.has(object)) {
      throw new ActionValueError(path, "refers to a value that already appeared in the payload");
    }
    seen.add(object);

    if (Array.isArray(object)) {
      const items: $ReadOnlyArray<mixed> = object as $FlowFixMe;
      for (let index = 0; index < items.length; index += 1) {
        stack.push({ value: items[index], path: `${path}[${String(index)}]`, depth: depth + 1 });
      }
      continue;
    }

    if (!isPlainObject(object)) {
      // A form gets its own sentence. It is the one prototype this grammar
      // does carry, just not here — a `FormData` is an argument of a call, and
      // this walk only ever sees the inside of one, or a result — and "is a
      // class instance" would send the reader looking for a class they did not
      // write.
      if (asFormData(object) != null) {
        throw new ActionValueError(
          path,
          "is a FormData, and a form may only be an argument of a call: never part of a value, " +
            "and never a result",
        );
      }
      throw new ActionValueError(
        path,
        "is a class instance, a Map, a Set, a Date, a React element or another object with a " +
          "prototype, and only arrays and plain objects cross to a server action",
      );
    }

    // Symbol keys are silently dropped by `JSON.stringify`, so a payload built
    // with one would arrive missing a property nobody could see was missing.
    if (Object.getOwnPropertySymbols(object).length > 0) {
      throw new ActionValueError(path, "has a symbol key, which has no spelling on the wire");
    }

    const record: { readonly [string]: mixed } = object as $FlowFixMe;
    for (const key of Object.getOwnPropertyNames(object)) {
      if (FORBIDDEN_KEYS.includes(key)) {
        throw new ActionValueError(`${path}.${key}`, "is a key this grammar never carries");
      }
      stack.push({ value: record[key], path: `${path}.${key}`, depth: depth + 1 });
    }
  }
}

/**
 * One form's entries, as pairs of strings, or a named failure.
 *
 * The count is checked as the entries are read rather than afterwards, so an
 * enormous form is refused before the whole of it has been copied. A `File`
 * entry is refused by name: an upload is not in this grammar and the field
 * that carried it is the useful half of saying so.
 */
function formEntries(form: FormData, label: string): Array<Array<string>> {
  const entries: Array<Array<string>> = [];
  for (const [name, value] of form.entries()) {
    if (entries.length >= MAX_FORM_ENTRIES) {
      throw new ActionValueError(
        label,
        `carries more than ${String(MAX_FORM_ENTRIES)} form fields`,
      );
    }
    if (name.length > MAX_FORM_NAME_LENGTH) {
      throw new ActionValueError(
        label,
        `has a form field name longer than ${String(MAX_FORM_NAME_LENGTH)} characters`,
      );
    }
    if (typeof value !== "string") {
      throw new ActionValueError(
        `${label} field \`${name}\``,
        "is a file, and a file cannot cross to a server action",
      );
    }
    entries.push([name, value]);
  }
  return entries;
}

/**
 * The request body for a call, or a named failure.
 *
 * The browser's half. Refusing here is what turns "the server answered 400"
 * into "argument 2.createdAt is a class instance", at the call site, with a
 * stack that reaches the component.
 *
 * A `FormData` argument becomes the envelope's `form` key and leaves `null` in
 * its own slot, so `args` stays a list of values of exactly the length the
 * call had. Two forms is a refusal rather than a choice: React passes one, and
 * an envelope that could carry several would need to say which is which in a
 * place that is not `at`.
 */
export function encodeActionArguments(args: $ReadOnlyArray<mixed>): string {
  if (args.length > MAX_ACTION_ARGUMENTS) {
    throw new ActionValueError(
      "the call",
      `passes ${String(args.length)} arguments, and an action takes at most ` +
        String(MAX_ACTION_ARGUMENTS),
    );
  }
  const values: Array<mixed> = [];
  let form: {| readonly at: number, readonly entries: Array<Array<string>> |} | null = null;
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    const label = `argument ${String(index + 1)}`;
    const submitted = asFormData(argument);
    if (submitted != null) {
      if (form != null) {
        throw new ActionValueError(label, "is a second form, and a call carries at most one");
      }
      form = { at: index, entries: formEntries(submitted, label) };
      values.push(null);
      continue;
    }
    checkActionValue(argument, label);
    values.push(argument);
  }
  return form == null ? JSON.stringify({ args: values }) : JSON.stringify({ args: values, form });
}

/**
 * The arguments a request body holds, or a named failure.
 *
 * The server's half, and the one that faces the network. The shape is exact —
 * one object, one key — because a payload with a key nobody reads is a payload
 * whose sender believed something about it that is not true, and because the
 * only way to keep this closed as it grows is to refuse anything that is not
 * this today.
 */
export function decodeActionArguments(text: string): Array<ActionArgument> {
  let parsed: mixed;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw new ActionValueError("the body", "is not JSON");
  }
  if (parsed == null || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new ActionValueError("the body", "is not a JSON object");
  }
  const keys: $ReadOnlyArray<string> = Object.getOwnPropertyNames(parsed);
  const named = keys.length === 1 ? keys[0] === "args" : keys.length === 2 && keys.includes("form");
  if (!named || !keys.includes("args")) {
    throw new ActionValueError("the body", 'has keys other than "args" and "form"');
  }
  const args: mixed = parsed.args;
  if (!Array.isArray(args)) {
    throw new ActionValueError("the body", 'has an "args" that is not an array');
  }
  if (args.length > MAX_ACTION_ARGUMENTS) {
    throw new ActionValueError(
      "the body",
      `passes more than ${String(MAX_ACTION_ARGUMENTS)} arguments`,
    );
  }
  for (let index = 0; index < args.length; index += 1) {
    checkActionValue(args[index], `argument ${String(index + 1)}`);
  }
  const decoded: Array<ActionArgument> = args as $FlowFixMe;
  if (keys.length === 2) {
    const at = decodeForm(parsed.form, decoded);
    decoded[at.index] = at.form;
  }
  return decoded;
}

/**
 * The envelope's `form`, as a `FormData` and the position it belongs at.
 *
 * Every field of the envelope is checked before anything is built, and the
 * slot it names must already hold `null`: `args` and `form` are two statements
 * about one call, and a payload that makes both about the same argument is a
 * payload whose sender believed something that is not true. The only thing
 * constructed is a `FormData`, from strings, and which constructor that is was
 * decided here rather than by the bytes.
 */
function decodeForm(
  candidate: mixed,
  args: $ReadOnlyArray<ActionArgument>,
): {| readonly index: number, readonly form: FormData |} {
  if (typeof FormData === "undefined") {
    throw new ActionValueError("the body", "carries a form, and this runtime has no FormData");
  }
  if (candidate == null || typeof candidate !== "object" || Array.isArray(candidate)) {
    throw new ActionValueError("the form", "is not a JSON object");
  }
  const keys: $ReadOnlyArray<string> = Object.getOwnPropertyNames(candidate);
  if (keys.length !== 2 || !keys.includes("at") || !keys.includes("entries")) {
    throw new ActionValueError("the form", 'has keys other than "at" and "entries"');
  }

  const at: mixed = candidate.at;
  if (typeof at !== "number" || !Number.isInteger(at) || at < 0 || at >= args.length) {
    throw new ActionValueError("the form", "names no argument of this call");
  }
  if (args[at] !== null) {
    throw new ActionValueError(
      "the form",
      `names argument ${String(at + 1)}, which the payload also gives a value`,
    );
  }

  const entries: mixed = candidate.entries;
  if (!Array.isArray(entries)) {
    throw new ActionValueError("the form", 'has an "entries" that is not an array');
  }
  if (entries.length > MAX_FORM_ENTRIES) {
    throw new ActionValueError(
      "the form",
      `carries more than ${String(MAX_FORM_ENTRIES)} form fields`,
    );
  }

  const form: FormData = new FormData();
  for (const entry of entries) {
    if (!Array.isArray(entry) || entry.length !== 2) {
      throw new ActionValueError("the form", "has an entry that is not a name and a value");
    }
    const [name, value] = entry;
    if (typeof name !== "string" || typeof value !== "string") {
      throw new ActionValueError("the form", "has an entry whose name or value is not a string");
    }
    if (name.length > MAX_FORM_NAME_LENGTH) {
      throw new ActionValueError(
        "the form",
        `has a form field name longer than ${String(MAX_FORM_NAME_LENGTH)} characters`,
      );
    }
    form.append(name, value);
  }
  return { index: at, form };
}

/**
 * The response body for a result, or a named failure.
 *
 * An action that returns nothing answers `{}` rather than `{"value":null}`:
 * JSON has no `undefined`, and turning one into `null` would make an action
 * declared `Promise<void>` resolve to something on the browser side.
 */
export function encodeActionResult(value: mixed): string {
  if (value === undefined) {
    return "{}";
  }
  checkActionValue(value, "the result");
  return JSON.stringify({ value });
}

/**
 * The result a response body holds, or a named failure.
 *
 * The same grammar in the other direction, and checked rather than trusted:
 * the browser is talking to whatever answered, which on a compromised network
 * is not the server. What it can be handed is therefore data and never an
 * object of somebody's choosing, exactly as on the way out.
 */
export function decodeActionResult(text: string): ActionValue | void {
  let parsed: mixed;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw new ActionValueError("the answer", "is not JSON");
  }
  if (parsed == null || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new ActionValueError("the answer", "is not a JSON object");
  }
  const keys: $ReadOnlyArray<string> = Object.getOwnPropertyNames(parsed);
  if (keys.length === 0) {
    return undefined;
  }
  if (keys.length !== 1 || keys[0] !== "value") {
    throw new ActionValueError("the answer", 'has keys other than "value"');
  }
  const value: mixed = parsed.value;
  checkActionValue(value, "the result");
  return value as $FlowFixMe;
}

/**
 * Whether `text` is the canonical spelling of an action id.
 *
 * Sixty-four lowercase hexadecimal characters, which is what
 * `uf_rsc::ActionId::to_hex` writes. Checked before the id reaches the table
 * so an oversized or repeated header cannot make the endpoint do work, and
 * hand-written rather than a regular expression because this reads a header a
 * client chose (`docs/security.md`, rule 5).
 */
export function isActionId(text: string): boolean {
  if (text.length !== 64) {
    return false;
  }
  for (let index = 0; index < text.length; index += 1) {
    const code = text.charCodeAt(index);
    const digit = code >= 0x30 && code <= 0x39;
    const lower = code >= 0x61 && code <= 0x66;
    if (!digit && !lower) {
      return false;
    }
  }
  return true;
}
