"use client";
// @flow
//
// The client half of the fixture: state, an event handler, a server action, a
// form, and a marker string that has to survive into the browser bundle.
//
// `"use client"` makes this a client bundle root, so `crates/uf_rsc` reports a
// boundary at `app/counter/$page.js` and every module above it stays in the
// client bundle. That is the assertion the split has to keep true: dropping a
// route the browser does not need must not drop the one it does.
//
// The import of `../_actions/tally.js` is the other half. On the server it is
// the module; in the browser it is two `createServerReference` calls, so this
// module ships and the one it imports does not. Clicking `record` posts one id
// to this page's own URL and renders what the server answered; submitting the
// form posts the other, with the form's fields beside the previous state.
//
// The form is React 19's own shape and nothing uf invented: `useActionState`
// calls the action with the previous state and the `FormData`, `<form
// action={…}>` supplies that `FormData`, and `useFormStatus` reads the submit
// that is in flight from inside the form. What makes it work against a server
// is that a reference is an ordinary async function — see
// `packages/router/action.js`.

import * as React from "@uniflowed/react";
import { useActionState, useState } from "@uniflowed/react";
import { useFormStatus } from "react-dom";

import { expect, it } from "@uniflowed/test";

import { type NoteState, recordCount, submitNote } from "../_actions/tally.js";

/** The string that proves this module is in a bundle. */
export const COUNTER_MARKER: string = "counter-marker-the-browser-needs-this";

/**
 * An in-source test in the one module of this fixture the browser certainly
 * gets.
 *
 * The marker above has to be in `dist/assets/*.js` and this one has to not be,
 * from the same file and the same build — which is the whole of what
 * ubugeeei-prod/uf#518 asks to be proved rather than assumed. `uf` compiles
 * `import.meta.uf.test` to `void 0` for every transform except the ones
 * `uf test` starts, so what the bundler is handed is `if (void 0) { … }`.
 *
 * A separate string rather than reusing `COUNTER_MARKER`: a test that looked
 * for the same literal twice could not tell "the block was removed" from "the
 * block was kept and the marker appears anyway".
 *
 * The bindings come from the top-level import of `@uniflowed/test` above,
 * which is the form uf recommends and the harder one to get right: the block
 * folding away is not enough on its own, because an unused import of a package
 * that spawns processes and reads `node:module` would still have to be shaken
 * out of a browser bundle. `@uniflowed/test` declares `sideEffects: false` so
 * that it can be, and this fixture is where that stops being a claim.
 */
if (import.meta.uf.test) {
  it("counts up", () => {
    expect("in-source-marker-no-build-may-ship-this").toBe(COUNTER_MARKER);
  });
}

/** What the note is before anything has been submitted. */
const NO_NOTE: NoteState = { saved: null, problem: null };

/**
 * The submit button, which is a component because `useFormStatus` has to be.
 *
 * The hook reads the state of the form it is *inside*, so a button that knows
 * whether its own submit is in flight cannot be the component that renders the
 * `<form>`. That is React's rule rather than uf's, and the fixture follows it
 * because an application has to.
 */
component SaveNote() {
  const { pending } = useFormStatus();
  return (
    <button type="submit" disabled={pending} data-saving={pending ? "yes" : "no"}>
      save note
    </button>
  );
}

export default component Counter() {
  const [count, setCount] = useState<number>(0);
  const [recorded, setRecorded] = useState<string>("");
  const [note, saveNote] = useActionState<NoteState, FormData>(submitNote, NO_NOTE);
  return (
    <div>
      <output data-marker={COUNTER_MARKER}>{count}</output>
      <button type="button" onClick={() => setCount(count + 1)}>
        add one
      </button>
      <button
        type="button"
        onClick={() => {
          void recordCount(count).then((answer) => {
            setRecorded(`${String(answer.total)} ${answer.visitor}`);
          });
        }}
      >
        record
      </button>
      <output data-recorded>{recorded}</output>
      <form action={saveNote}>
        <input name="note" maxLength={80} defaultValue="" />
        <SaveNote />
        <output data-note>{note.saved ?? note.problem ?? ""}</output>
      </form>
    </div>
  );
}
