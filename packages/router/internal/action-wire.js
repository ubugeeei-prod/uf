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
// **A server action's arguments are plain JSON data and nothing else.**
//
// One JSON object, `{"args": [...]}`, at most `MAX_ACTION_BODY_BYTES` of valid
// UTF-8, holding at most `MAX_ACTION_ARGUMENTS` values, nested at most
// `MAX_ACTION_DEPTH` deep, with at most `MAX_ACTION_VALUES` values in total.
// Each of those values is `null`, a boolean, a finite number, a string, an
// array of them, or a plain object whose keys are ordinary strings. The result
// travels back under exactly the same grammar, plus `undefined` for an action
// that returns nothing.
//
// Nothing in a payload can name a function, a module, a class, a prototype, a
// React element, an id or a reference, and nothing in it is revived into an
// object the sender chose. `decodeActionArguments` produces values a
// `JSON.parse` already produced; what this adds is the refusal of everything
// `JSON.parse` would have let through — which is the whole of what a decoder
// has to get right.
//
// # What is deliberately absent, and why
//
// * **A reference format.** React's Flight payload can carry a reference to a
//   client module, a promise, or an element, and a decoder that reconstructs
//   those is a decoder that constructs attacker-chosen objects. uf has no such
//   payload (ubugeeei-prod/uf#252) and this grammar is not the place to grow
//   one quietly.
// * **Class instances, `Map`, `Set`, `Date`, `RegExp`, typed arrays.** Each
//   would need a tag in the payload saying which constructor to call, and a
//   tag naming a constructor is the oracle every deserialisation CVE is made
//   of. An action that wants a date takes an ISO string and parses it, where
//   the parse is the application's and is checked.
// * **`FormData` and `<form action={fn}>`.** Multipart parsing is its own
//   attack surface with its own bounds, and a form post is a *simple* CORS
//   request — it reaches a server with the visitor's cookies and no preflight.
//   Both are worth having and neither is worth having by accident; see the
//   security note in `./action-endpoint.js`.
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
 * The request body for a call, or a named failure.
 *
 * The browser's half. Refusing here is what turns "the server answered 400"
 * into "argument 2.createdAt is a class instance", at the call site, with a
 * stack that reaches the component.
 */
export function encodeActionArguments(args: $ReadOnlyArray<mixed>): string {
  if (args.length > MAX_ACTION_ARGUMENTS) {
    throw new ActionValueError(
      "the call",
      `passes ${String(args.length)} arguments, and an action takes at most ` +
        String(MAX_ACTION_ARGUMENTS),
    );
  }
  for (let index = 0; index < args.length; index += 1) {
    checkActionValue(args[index], `argument ${String(index + 1)}`);
  }
  return JSON.stringify({ args });
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
export function decodeActionArguments(text: string): Array<ActionValue> {
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
  if (keys.length !== 1 || keys[0] !== "args") {
    throw new ActionValueError("the body", 'has keys other than "args"');
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
  return args as $FlowFixMe;
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
