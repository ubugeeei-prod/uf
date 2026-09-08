"use client";
// @flow
//
// The client half of the fixture: state, an event handler, a server action, a
// form, and a marker string that has to survive into the browser bundle.
//
// `"use client"` makes this a client bundle root, so `crates/uf_rsc` reports a
// boundary at `app/counter/_uf.page.js` and every module above it stays in the
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

import { type NoteState, recordCount, submitNote } from "../_actions/tally.js";

/** The string that proves this module is in a bundle. */
export const COUNTER_MARKER: string = "counter-marker-the-browser-needs-this";

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
