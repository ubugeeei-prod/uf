"use server";
// @flow
//
// The server action the counter calls.
//
// A `"use server"` module: every export is a function the browser may invoke
// by id over the network, and nothing in this file is code the browser has.
// `@uniflowed/vite` gives the client bundle one `createServerReference` per
// export of this module, so what the button holds is an id and a `fetch`.
//
// Both of the things this file reaches for are the point of it. `cookies()`
// comes from `@uniflowed/server`, which imports `node:async_hooks` — the
// import ubugeeei-prod/uf#252 names as the one a page that calls it used to
// put in the browser's graph with nothing to stop it. `./ledger.js` is the
// module an action reaches that has no directive of its own and is server code
// anyway, because the only thing that imports it is this.
//
// Two exports, because there are two ways a browser reaches one. `recordCount`
// is a click: the client component calls it with a number. `submitNote` is a
// form: React hands it the previous state and a `FormData`, and the validation
// that turns those raw fields into a value the application may use happens
// here, on the server, where a caller cannot skip it.
//
// `crates/uf_cli/tests/vite.rs` builds this project and asserts that neither
// marker below is in `dist/assets/*.js`, that `node:async_hooks` is not
// either, and that both references that replaced this module are.

import { cookies } from "@uniflowed/server";
import { maxLength, minLength, object, pipe, safeParse, string, trim } from "@uniflowed/validator";
import { put } from "@uniflowed/validator/plain-object";

import { LEDGER_MARKER, tallyFor } from "./ledger.js";

/**
 * The string that proves this module reached a bundle it should not have.
 *
 * Not exported: a `"use server"` module may only export async functions, and
 * the RSC graph reports anything else. A string literal in a function body
 * survives minification exactly as an exported one would, which is all the
 * assertion needs.
 */
const TALLY_MARKER = "tally-marker-only-the-server-runs-this";

/** Record a count and answer with what the server made of it. */
export async function recordCount(count: number): Promise<{|
  readonly total: number,
  readonly marker: string,
  readonly visitor: string,
|}> {
  return {
    total: tallyFor(count),
    marker: `${TALLY_MARKER}/${LEDGER_MARKER}`,
    // Proof that an action runs inside the request the host began: outside one
    // this throws rather than answering `null`.
    visitor: cookies().get("visitor") ?? "anonymous",
  };
}

/**
 * What one submitted note is allowed to be.
 *
 * The schema is the only description of a note that exists, and it is on the
 * server: `<input required maxlength="80">` is a convenience for the person
 * typing and is not a check, because the request that reaches this function
 * was not necessarily made by that document.
 */
const Note = object({ note: pipe(string(), trim(), minLength(1), maxLength(80)) });

/** What `useActionState` holds between submits. */
export type NoteState = {| readonly saved: string | null, readonly problem: string | null |};

/**
 * The form the counter submits: validated here, answered as state.
 *
 * The shape React's `useActionState` calls an action with — the previous
 * state, then the `FormData` — and the shape the wire carries it in: one form
 * per call, beside the values rather than inside one. A field that is missing,
 * empty or too long is a `problem` on the state rather than an exception,
 * because the endpoint answers a thrown action with a `500` and nothing in it,
 * and "your note is too long" is not an internal error.
 */
export async function submitNote(previous: NoteState, form: FormData): Promise<NoteState> {
  // Two things about this loop, and neither is decoration.
  //
  // The `typeof` is the narrowing Flow asks for — a `FormDataEntryValue` is
  // `string | File` — and the branch it guards cannot be taken here, because
  // the wire refuses a file on the way out and on the way in.
  //
  // `put` rather than `fields[name] = value`, because a form field may
  // legitimately be named `__proto__` and a plain assignment of that key runs
  // the legacy setter instead of adding a property. `@uniflowed/validator`
  // reaches for the same helper everywhere it builds an object out of keys
  // that came from outside, which is the only reason there is one to import.
  const fields: { [string]: string } = {};
  for (const [name, value] of form.entries()) {
    if (typeof value === "string") {
      put(fields, name, value);
    }
  }

  const parsed = safeParse(Note, fields);
  if (!parsed.ok) {
    // The previous `saved` survives a rejected submit, which is the reason
    // `useActionState` hands an action the previous state at all.
    return { saved: previous.saved, problem: "a note is 1 to 80 characters" };
  }
  const visitor = cookies().get("visitor") ?? "anonymous";
  return {
    saved: `${parsed.value.note}/${String(tallyFor(parsed.value.note.length))}/${visitor}/${TALLY_MARKER}`,
    problem: null,
  };
}
