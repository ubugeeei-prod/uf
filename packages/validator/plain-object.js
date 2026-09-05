// @flow
//
// `@uniflowed/validator/plain-object`: reading and writing an object whose
// keys came from outside.
//
// Three functions, and they are separate from every schema that uses them
// because they are the package's whole answer to one question: what happens
// when a payload contains a key that means something to JavaScript.
//
// `{"__proto__": {"isAdmin": true}}` is valid JSON, and it is what an attacker
// sends. `out[key] = value` runs the legacy `__proto__` setter rather than
// adding a property, so the parsed object would come back with a prototype the
// attacker chose and every `record.isAdmin` downstream would read `true` from
// a property nobody ever validated. Reading is the mirror image: `input[key]`
// finds inherited properties, so a shape asking for `constructor` would be
// handed `Object`.
//
// `object.js`, `collection.js`, `union.js` and `issue.js` all build or read an
// object out of untrusted input, and every one of them goes through here
// rather than keeping its own copy of the rule. That is the reason this is a
// module and not three helpers at the top of the object parser: the defence is
// only a defence if there is exactly one of it.

/** Whether `value` is an object a shape or a record could be read from. */
export function isPlainObject(value: mixed): boolean {
  return value != null && typeof value === "object" && !Array.isArray(value);
}

/**
 * `value` as something with string keys.
 *
 * The single object boundary in the package. Every caller has already asked
 * [`isPlainObject`], and every field that leaves a schema went through that
 * schema first, so the `mixed` values this exposes are narrowed before they
 * reach an output type.
 */
export function plainRecord(value: mixed): { readonly [string]: mixed, ... } {
  // $FlowFixMe[incompatible-type] guarded by `isPlainObject` at every call.
  return value as { readonly [string]: mixed, ... };
}

/**
 * The own keys of `value`, and nothing inherited.
 *
 * `Object.keys` already skips the prototype chain, which is why it is here
 * rather than `for (const key in value)`.
 */
export function ownKeys(value: { readonly [string]: mixed, ... }): $ReadOnlyArray<string> {
  return Object.keys(value);
}

/**
 * Read one key, without falling through to the prototype.
 *
 * `record.constructor` is `Object` on every object in the language; a shape
 * with a `constructor` field would otherwise be handed a function and report
 * that the payload was fine. It also makes `object(shape)` answer the same way
 * for `{}` and for a class instance with getters on its prototype, which is
 * the sort of difference that is discovered in production.
 *
 * `Object.hasOwn` on every field read costs about 13% of a field-dense parse,
 * measured on the workload in `index.js`. That is the price of the paragraph
 * above and it is being paid deliberately: a payload out of `JSON.parse` has
 * only own keys, so the check earns nothing there, and it earns everything the
 * first time somebody hands a schema an object they built themselves.
 */
export function ownValue(source: { readonly [string]: mixed, ... }, key: string): mixed {
  return Object.hasOwn(source, key) ? source[key] : undefined;
}

/**
 * Write one parsed field into the object being built.
 *
 * `__proto__` is the only key that needs `defineProperty`, and it is worth
 * knowing why rather than reaching for it on every field. `Object.prototype`
 * has exactly one accessor on it — `__proto__` — and assignment to a key an
 * accessor owns runs the setter instead of adding a property. Every other
 * inherited name (`constructor`, `toString`, `valueOf`) is a *data* property,
 * and assigning to one of those shadows it with an own property on the
 * receiver, which is what a parsed field is supposed to be.
 *
 * So one string comparison is the whole defence against a hostile payload, and
 * `defineProperty` is the slow path for the one key that needs it. That is not
 * a micro-optimisation: `defineProperty` on every field made a thousand-record
 * parse three times slower than the same parse with an assignment in it — more
 * than the entire rest of the walk cost — and the measurement is in
 * `index.js`.
 *
 * What this does not defend against is an `Object.prototype` that some other
 * code has already given a setter to. Nothing in a parser can: a process whose
 * `Object.prototype` is writable by an attacker has lost, and every read the
 * consumer makes afterwards goes through the same polluted object.
 */
export function put<Value>(out: { [string]: Value, ... }, key: string, value: Value): void {
  if (key !== "__proto__") {
    out[key] = value;
    return;
  }
  Object.defineProperty(out, key, {
    value,
    writable: true,
    enumerable: true,
    configurable: true,
  });
}
