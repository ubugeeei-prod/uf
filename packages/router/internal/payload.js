// @flow
//
// Internal to `@uniflowed/router`: the payload, and what may be in it.
//
// A document uf streams has always carried one thing the browser reads back:
// the loader's answer, in `<script id="__uf_data" type="application/json">`.
// That element is written once, from a value that is complete by the time it
// is written, so everything in it had to have resolved before the byte after
// it could be sent. A page whose data is one slow thing and four fast ones
// therefore waited for the slow one to say anything about the other four, and
// a promise anywhere in that value was `JSON.stringify`'d to `{}` — silently,
// which is the worse half.
//
// This module is the format that stops it waiting. It is **Flight-shaped** in
// the one sense that matters: a payload is not a value, it is a sequence of
// numbered *rows*, the first of which may name rows that have not been written
// yet. Row 0 is the model — the loader's answer with each unresolved value
// replaced by a reference — and each later row is one of those values,
// arriving in whatever order it resolved in, in a `<script>` React streams
// into the document at the moment it settles.
//
//   0  {"user":{"name":"ada"},"comments":"$P1","related":"$P2"}
//   2  {"value":[{"id":7}]}
//   1  {"value":[{"body":"…"}]}
//
// Two rows out of order, because row 2 resolved first, and that is the whole
// point: the reader has the page and the user's name while the comments are
// still being fetched, and the boundary around the comments resolves on its
// own rather than behind the slowest thing on the page.
//
// # One reference kind, and the reason there is only one
//
// `./action-wire.js` says, under "What is deliberately absent", that React's
// Flight payload can carry a reference to a client module, a promise, or an
// element, and that "a decoder that reconstructs those is a decoder that
// constructs attacker-chosen objects" — so the action grammar has no reference
// format and "this grammar is not the place to grow one quietly". This is the
// place, and it grows exactly one:
//
//   `"$P<n>"`  the value written by row `n` of this same payload.
//
// A row reference names a *position in this payload*, and it is resolved by
// this module's own reader with a promise this module made. Nothing in a
// payload names a constructor, a module, a function, an export or a class, and
// no string in one is looked up in any registry. That is the difference
// between the reference this format has and the two it does not: a module
// reference and an element reference are decoded by *calling* something the
// payload named, and the payload the client re-renders a *tree* from needs
// both. Those wait for ubugeeei-prod/uf#519's other half, which needs a second
// module graph before it needs a format. This half is the framing and the
// streaming, and it is written so that adding a tag later is a change to a
// closed list rather than to a decoder that already passes strings through:
// an unrecognised `$` string is **refused**, because a decoder that ignored
// `"$X1"` today would accept it silently on the day `$X` means something.
//
// # What the model may hold is unchanged, and that is deliberate
//
// Everything else in a payload is what `JSON.stringify` carries, because that
// is what the loader data element has always been. Narrowing it to the closed
// grammar `./action-wire.js` applies to a server action would be a change to a
// contract this format is not about — and it would be a change made in a
// throw, inside a loader, on somebody whose page worked yesterday. An action's
// arguments arrive from the network and are somebody else's bytes; a loader's
// answer is the application's own value on its way out, and the two do not
// need the same answer.
//
// So the encoder does exactly two things: it replaces promises with references
// and it escapes strings that would read as one. A `Date`, a class with a
// `toJSON`, an `undefined` property, a `NaN` — each crosses exactly as it did
// before this file existed, which is to say as `JSON.stringify` renders it.
// The one shape that is outside this and says so is a cycle: the walk is
// bounded by [`MAX_PAYLOAD_DEPTH`], and `JSON.stringify` threw on one anyway.
//
// The escape is applied to every string the walk *reaches*, which is every
// string inside a plain object or an array. A string that only appears because
// something's `toJSON` produced it is not reached and not escaped — so a
// `toJSON` returning a string that begins with `$` is the one value this
// format cannot round-trip, and it is named here rather than left to be found.
//
// # Nothing changes for a payload with nothing deferred
//
// Both walks answer with the value they were handed when there is nothing to
// do — no promise anywhere, no string starting with `$`. So a page whose
// loader returns ordinary data produces the same bytes it produced before this
// module existed, and the browser hands the application the object
// `JSON.parse` made rather than a copy of it. That is worth more than the
// walk it saves: it is what makes "the payload changed nothing here" a
// property rather than a hope.
//
// # Why both sides run this file
//
// The same reason `./action-wire.js` gives: two implementations of one grammar
// is how the two come to disagree. Here it is load-bearing beyond that. The
// row `<script>` is rendered by React *on both sides* — the server writes it
// from the promise it resolved, the browser writes it from the value it read
// out of that same element — so the two renders have to produce the same bytes
// or the page is a hydration mismatch. They do, because both call
// [`payloadJson`] on a value that has been through one `JSON.parse`, and
// `JSON.stringify` is stable over its own output.
//
// Pure: no imports, no platform APIs beyond `JSON`, so the browser's half of
// `@uniflowed/router` can reach it without reaching anything server-only. The
// document half — finding the row elements and noticing the ones that have not
// arrived yet — is `./payload-rows.js`, which is the only part that needs a DOM.

/** The attribute naming a late row's script element. */
export const PAYLOAD_ROW_ATTRIBUTE: string = "data-uf-row";

/**
 * The prefix every reference in this format starts with.
 *
 * One character, so that escaping a string that starts with it costs one
 * character too. React's own Flight payload uses the same one, and a format
 * that reads as Flight does to somebody who knows Flight is worth more than a
 * prefix nobody has ever seen.
 */
const REFERENCE_PREFIX = "$";

/** The tag that makes a reference a row reference. `$P1` is "the value of row 1". */
const ROW_TAG = "P";

/**
 * Most rows one payload may have.
 *
 * A ceiling on the number of `<script>` elements one document can be made to
 * carry, and on the number of promises the browser holds open waiting for
 * them. A loader that defers more than this is a loader that wants one request
 * per thing rather than one page.
 */
export const MAX_PAYLOAD_ROWS: number = 64;

/**
 * Deepest nesting either walk will follow.
 *
 * A guard rather than a policy. `JSON.stringify` refuses a cycle by throwing,
 * and the walks below would follow one forever, so the depth is what keeps the
 * two failing the same way. Deep enough that no data shaped like data reaches
 * it: `MAX_ACTION_DEPTH` is 24, and this is not the place to be stricter than
 * the wire an application already has.
 */
export const MAX_PAYLOAD_DEPTH: number = 64;

/** A value that could not be encoded, and where in the loader's answer it was. */
export class PayloadValueError extends Error {
  /** Where the offending value sat, e.g. `data.filters`. */
  path: string;

  constructor(path: string, reason: string) {
    super(`@uniflowed/router: ${path} ${reason}.`);
    this.name = "PayloadValueError";
    this.path = path;
  }
}

/**
 * A row that said the value failed, carried back as a rejection.
 *
 * The class exists so the reading side can rewrite the row it read without
 * having to guess. A row is rendered by React on *both* sides, so whatever the
 * browser puts in the element has to be what the server put there — and the
 * server's choice of words depends on whether it is a development build. This
 * carries the row's own text past that decision: the browser echoes `wire`
 * rather than deciding again, and the two agree whichever build wrote which.
 */
export class PayloadRowError extends Error {
  /** Exactly what the row's `error` said. */
  wire: string;

  constructor(wire: string) {
    super(wire);
    this.name = "PayloadRowError";
    this.wire = wire;
  }
}

/** One value the model referred to and that has not resolved yet. */
export type PendingRow = {|
  /** Row number, 1-based; row 0 is the model itself. */
  readonly id: number,
  /** The promise whose settlement writes the row. */
  readonly value: Promise<mixed>,
|};

/** The model, and every row it named. */
export type EncodedPayload = {|
  /** Row 0: the value with each promise replaced by a `"$P<n>"` reference. */
  readonly model: mixed,
  /** The promises those references name, in ascending id order. */
  readonly rows: $ReadOnlyArray<PendingRow>,
|};

/**
 * What one late row's script says.
 *
 * Two shapes rather than one so that a rejected promise is a rejected promise
 * on the other side too, instead of a value that never arrives and a boundary
 * that spins. What is in the message is decided by the caller rather than here
 * — see `internal/runtime.js`, which sends the server's own words only where
 * `import.meta.hot` says a developer is reading them.
 *
 * One exact object with two optional fields rather than a union of two, and
 * the reason is `JSON.stringify`: the value that gets written has to have
 * exactly one key in it, so the type the writer holds has to be the one that
 * can express either. [`parseRowMessage`] enforces "exactly one" on the way
 * back in, which is the direction where it is a claim about somebody else's
 * bytes rather than about uf's own.
 */
export type PayloadRowMessage = {|
  readonly value?: mixed,
  readonly error?: string,
|};

/**
 * The prototype every object literal has, captured rather than named.
 *
 * `./action-wire.js` explains the choice; the same one is made here so the two
 * agree about what a plain object is. Only a plain object and an array are
 * walked into — everything else is `JSON.stringify`'s business, as it was.
 */
const PLAIN_PROTOTYPE: mixed = Object.getPrototypeOf({});

function isPlainObject(value: mixed): boolean {
  if (value == null || typeof value !== "object") {
    return false;
  }
  const prototype: mixed = Object.getPrototypeOf(value);
  return prototype === PLAIN_PROTOTYPE || prototype === null;
}

/**
 * The value as a promise, or `null`.
 *
 * A `then` method and nothing else, which is the one duck-type this file makes
 * and the one it has to: a loader's promise may come from a different realm
 * than the one this module was loaded in, and `instanceof Promise` is false
 * across realms. It is safe in a way it would not be in `./action-wire.js`
 * because a thenable here is never *reconstructed* — it is awaited, and what
 * it produces is written by `JSON.stringify` like any other value.
 */
function asThenable(value: mixed): Promise<mixed> | null {
  if (value == null || (typeof value !== "object" && typeof value !== "function")) {
    return null;
  }
  const then: mixed = (value: $FlowFixMe).then;
  return typeof then === "function" ? (value: $FlowFixMe) : null;
}

/** Whether a string would be read as a reference and so has to be escaped. */
function isReferenceLike(value: mixed): boolean {
  return typeof value === "string" && value.startsWith(REFERENCE_PREFIX);
}

/**
 * Write a key without letting `__proto__` mean what assignment makes it mean.
 *
 * `JSON.parse` gives `__proto__` an *own* data property; `target[key] = value`
 * calls the setter on `Object.prototype` and changes the object's prototype
 * instead. Rebuilding a parsed object with plain assignment would therefore
 * turn a payload that was inert into prototype pollution — a hazard these
 * walks would introduce rather than inherit. `defineProperty` is what
 * `JSON.parse` does, spelled out.
 */
function setKey(target: { [string]: mixed }, key: string, value: mixed): void {
  if (key === "__proto__") {
    Object.defineProperty(target, key, {
      value,
      writable: true,
      enumerable: true,
      configurable: true,
    });
    return;
  }
  target[key] = value;
}

/** One entry of an explicit walk stack, with the slot to write the result into. */
type Frame = {|
  readonly value: mixed,
  readonly path: string,
  readonly depth: number,
  readonly emit: (encoded: mixed) => void,
|};

/**
 * Whether anything in `root` needs encoding at all.
 *
 * A promise, or a string that would read as a reference. Answering `false`
 * here is what lets [`encodePayload`] hand back the value it was given — see
 * the header on why that matters more than the walk it saves.
 *
 * An explicit stack rather than recursion, for the reason `checkActionValue`
 * gives: nesting is the application's choice, and a recursive walk over it is
 * a stack overflow with somebody's hand on the depth.
 */
function needsEncoding(root: mixed, label: string): boolean {
  const stack: Array<{| value: mixed, path: string, depth: number |}> = [
    { value: root, path: label, depth: 0 },
  ];
  while (stack.length > 0) {
    const frame = stack.pop();
    if (frame == null) {
      break;
    }
    const { value, path, depth } = frame;
    if (depth > MAX_PAYLOAD_DEPTH) {
      throw new PayloadValueError(path, `is nested deeper than ${String(MAX_PAYLOAD_DEPTH)}`);
    }
    if (isReferenceLike(value) || asThenable(value) != null) {
      return true;
    }
    if (Array.isArray(value)) {
      for (let index = 0; index < value.length; index += 1) {
        stack.push({ value: value[index], path: `${path}[${String(index)}]`, depth: depth + 1 });
      }
      continue;
    }
    if (isPlainObject(value)) {
      const source: { +[string]: mixed } = (value: $FlowFixMe);
      for (const key of Object.keys(source)) {
        stack.push({ value: source[key], path: `${path}.${key}`, depth: depth + 1 });
      }
    }
  }
  return false;
}

/**
 * Split a value into the model that goes out now and the rows that follow.
 *
 * The walk is **ordered**: children are visited in array order and in
 * `Object.keys` order, so the id a promise gets is a function of where it sits
 * in the model and of nothing else.
 *
 * That determinism is what lets the browser encode the same model to the same
 * bytes, which is what lets both sides render the row `<script>`. It holds
 * across a `JSON.parse` because parsing preserves both orders, and it is why
 * ids come from this walk rather than from the order the promises were created
 * in — which is a fact about the loader, not about the data, and would differ
 * between the two sides.
 */
export function encodePayload(root: mixed, label: string): EncodedPayload {
  if (!needsEncoding(root, label)) {
    return { model: root, rows: [] };
  }

  const rows: Array<PendingRow> = [];
  let model: mixed = null;
  const stack: Array<Frame> = [
    {
      value: root,
      path: label,
      depth: 0,
      emit: (encoded) => {
        model = encoded;
      },
    },
  ];

  while (stack.length > 0) {
    const frame = stack.pop();
    if (frame == null) {
      break;
    }
    const { value, path, depth, emit } = frame;
    if (depth > MAX_PAYLOAD_DEPTH) {
      throw new PayloadValueError(path, `is nested deeper than ${String(MAX_PAYLOAD_DEPTH)}`);
    }

    const thenable = asThenable(value);
    if (thenable != null) {
      if (rows.length >= MAX_PAYLOAD_ROWS) {
        throw new PayloadValueError(label, `defers more than ${String(MAX_PAYLOAD_ROWS)} values`);
      }
      const id = rows.length + 1;
      rows.push({ id, value: thenable });
      emit(`${REFERENCE_PREFIX}${ROW_TAG}${String(id)}`);
      continue;
    }
    if (isReferenceLike(value)) {
      emit(REFERENCE_PREFIX + String(value));
      continue;
    }
    if (Array.isArray(value)) {
      const encoded: Array<mixed> = new Array(value.length).fill(null);
      emit(encoded);
      // Pushed in reverse so that popping visits index 0 first: the id a
      // promise is given has to follow the order the payload reads in.
      for (let index = value.length - 1; index >= 0; index -= 1) {
        stack.push({
          value: value[index],
          path: `${path}[${String(index)}]`,
          depth: depth + 1,
          emit: (child) => {
            encoded[index] = child;
          },
        });
      }
      continue;
    }
    if (!isPlainObject(value)) {
      // A `Date`, a `Map`, a class instance, a number, `undefined`: whatever
      // `JSON.stringify` would have made of it, unchanged. See the header.
      emit(value);
      continue;
    }
    const source: { +[string]: mixed } = (value: $FlowFixMe);
    const encoded: { [string]: mixed } = {};
    emit(encoded);
    const keys = Object.keys(source);
    for (let index = keys.length - 1; index >= 0; index -= 1) {
      const key = keys[index];
      stack.push({
        value: source[key],
        path: `${path}.${key}`,
        depth: depth + 1,
        emit: (child) => {
          setKey(encoded, key, child);
        },
      });
    }
  }

  return { model, rows };
}

/**
 * Encode what one row resolved to.
 *
 * The same walk, and then the one rule a row has that the model does not: it
 * may not itself defer. Row ids are handed out by a single walk of the model,
 * which is the only structure both sides hold before anything has resolved, so
 * a row that could add ids as it settled would be a numbering that depends on
 * the order things finished in — and the browser, which re-renders the row
 * element from the value it read, would number them differently.
 *
 * Reusing the encoder and refusing the rows it found is the shortest honest
 * spelling: one walk, one escape rule, and no second implementation to drift
 * from the first.
 */
export function encodeRowValue(root: mixed, label: string): mixed {
  const { model, rows } = encodePayload(root, label);
  if (rows.length > 0) {
    throw new PayloadValueError(label, "resolved to a value that is itself deferred");
  }
  return model;
}

/** How a decoder is told what a reference stands for. */
export type RowResolver = (id: number) => mixed;

/**
 * Rebuild a value from a model, with every reference replaced.
 *
 * `resolve` is handed a row id and answers with whatever should stand in the
 * slot — a promise, on the reading side, which is the whole point. A callback
 * rather than a map, so a reader can create the promise on first sight of a
 * reference and never for a row nothing refers to.
 *
 * A model with no `$` string in it is handed straight back, so the application
 * gets the object `JSON.parse` made. See the header.
 */
export function decodePayload(root: mixed, resolve: RowResolver, label: string): mixed {
  if (!hasReference(root, label)) {
    return root;
  }

  let out: mixed = null;
  const stack: Array<Frame> = [
    {
      value: root,
      path: label,
      depth: 0,
      emit: (value) => {
        out = value;
      },
    },
  ];

  while (stack.length > 0) {
    const frame = stack.pop();
    if (frame == null) {
      break;
    }
    const { value, path, depth, emit } = frame;
    if (depth > MAX_PAYLOAD_DEPTH) {
      throw new PayloadValueError(path, `is nested deeper than ${String(MAX_PAYLOAD_DEPTH)}`);
    }

    if (isReferenceLike(value)) {
      emit(decodeReference(String(value), path, resolve));
      continue;
    }
    if (Array.isArray(value)) {
      const rebuilt: Array<mixed> = new Array(value.length).fill(null);
      emit(rebuilt);
      for (let index = value.length - 1; index >= 0; index -= 1) {
        stack.push({
          value: value[index],
          path: `${path}[${String(index)}]`,
          depth: depth + 1,
          emit: (child) => {
            rebuilt[index] = child;
          },
        });
      }
      continue;
    }
    if (!isPlainObject(value)) {
      emit(value);
      continue;
    }
    const source: { +[string]: mixed } = (value: $FlowFixMe);
    const rebuilt: { [string]: mixed } = {};
    emit(rebuilt);
    const keys = Object.keys(source);
    for (let index = keys.length - 1; index >= 0; index -= 1) {
      const key = keys[index];
      stack.push({
        value: source[key],
        path: `${path}.${key}`,
        depth: depth + 1,
        emit: (child) => {
          setKey(rebuilt, key, child);
        },
      });
    }
  }

  return out;
}

/** Whether a model holds anything the decoder has to look at. */
function hasReference(root: mixed, label: string): boolean {
  const stack: Array<{| value: mixed, path: string, depth: number |}> = [
    { value: root, path: label, depth: 0 },
  ];
  while (stack.length > 0) {
    const frame = stack.pop();
    if (frame == null) {
      break;
    }
    const { value, path, depth } = frame;
    if (depth > MAX_PAYLOAD_DEPTH) {
      throw new PayloadValueError(path, `is nested deeper than ${String(MAX_PAYLOAD_DEPTH)}`);
    }
    if (isReferenceLike(value)) {
      return true;
    }
    if (Array.isArray(value)) {
      for (let index = 0; index < value.length; index += 1) {
        stack.push({ value: value[index], path: `${path}[${String(index)}]`, depth: depth + 1 });
      }
      continue;
    }
    if (isPlainObject(value)) {
      const source: { +[string]: mixed } = (value: $FlowFixMe);
      for (const key of Object.keys(source)) {
        stack.push({ value: source[key], path: `${path}.${key}`, depth: depth + 1 });
      }
    }
  }
  return false;
}

/**
 * One `$` string: an escaped literal, or a row reference.
 *
 * The unrecognised case throws. See the header: a decoder that passed `"$X1"`
 * through would be one that accepts a tag it does not implement, from a
 * payload it did not write.
 */
function decodeReference(value: string, path: string, resolve: RowResolver): mixed {
  if (value.startsWith(REFERENCE_PREFIX + REFERENCE_PREFIX)) {
    return value.slice(1);
  }
  const id = rowId(value);
  if (id == null) {
    throw new PayloadValueError(path, "names a reference this payload format has no tag for");
  }
  return resolve(id);
}

/**
 * The row id carried by a streamed row element, or `null` when the attribute
 * is not one.
 *
 * This is the same grammar as the `$P<n>` reference without the `$P` tag:
 * digits only, in range. The browser reads row elements from a live document,
 * so accepting `Number.parseInt`'s looser spellings would let `1x` satisfy the
 * row the model named as `$P1`.
 */
export function payloadRowId(value: string): number | null {
  return parsePayloadRowId(value);
}

/**
 * The row a reference names, or `null` when the string is not one.
 *
 * Digits only, and read by hand rather than with `Number`, which accepts
 * `"1e3"`, `" 1"`, `"0x1"` and `"Infinity"` — four spellings of a row id this
 * format does not have.
 */
function rowId(value: string): number | null {
  if (!value.startsWith(REFERENCE_PREFIX + ROW_TAG)) {
    return null;
  }
  return parsePayloadRowId(value.slice(2));
}

function parsePayloadRowId(digits: string): number | null {
  if (digits.length === 0 || digits.length > 3) {
    return null;
  }
  for (let index = 0; index < digits.length; index += 1) {
    const code = digits.charCodeAt(index);
    if (code < 48 || code > 57) {
      return null;
    }
  }
  const id = Number.parseInt(digits, 10);
  return id >= 1 && id <= MAX_PAYLOAD_ROWS ? id : null;
}

/**
 * A payload row, or the model, as the text of a `<script type="application/json">`.
 *
 * `<` is escaped so a string holding `</script>` cannot end the element early,
 * and U+2028 and U+2029 because a JSON document is not JavaScript source but is
 * sometimes read as if it were. This is the escape `internal/runtime.js`
 * applied inline before there was a payload; it lives here now because the
 * model and every row have to be written the same way and there is no longer
 * only one of them.
 */
export function payloadJson(value: mixed): string {
  return JSON.stringify(value)
    .replace(/</g, "\\u003c")
    .replace(/\u2028/g, "\\u2028")
    .replace(/\u2029/g, "\\u2029");
}

/**
 * Read one late row's script text.
 *
 * Refuses anything that is not exactly one of the two shapes, and refuses a
 * message carrying both — a row is a value or a failure, never a choice the
 * reader has to make. The value goes through [`decodePayload`] with a resolver
 * that refuses every reference, which is where "a row may not itself be
 * deferred" is enforced on the reading side: ids are handed out by one walk of
 * the model, and a row that could add more would be a numbering that depends
 * on the order things finished in.
 */
export function parseRowMessage(text: string, id: number): PayloadRowMessage {
  const label = `row ${String(id)}`;
  let parsed: mixed;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw new PayloadValueError(label, "is not JSON");
  }
  if (!isPlainObject(parsed)) {
    throw new PayloadValueError(label, "is not a row message");
  }
  const message: { +[string]: mixed } = (parsed: $FlowFixMe);
  const hasValue = Object.prototype.hasOwnProperty.call(message, "value");
  const hasError = Object.prototype.hasOwnProperty.call(message, "error");
  if (hasValue === hasError) {
    throw new PayloadValueError(label, "must carry exactly one of `value` and `error`");
  }
  if (hasError) {
    const error = message.error;
    if (typeof error !== "string") {
      throw new PayloadValueError(label, "carries an `error` that is not a string");
    }
    return { error };
  }
  return {
    value: decodePayload(
      message.value,
      () => {
        throw new PayloadValueError(label, "refers to another row");
      },
      label,
    ),
  };
}
