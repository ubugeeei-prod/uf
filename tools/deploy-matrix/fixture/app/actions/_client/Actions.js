// @flow
"use client";

// The two ways the browser reaches a server action, on one page: a button that
// calls `add` through its client reference (the JSON path), and a form bound
// to `saveNote` through `useActionState`, which is a real HTML form before the
// page hydrates (the native form path, ubugeeei-prod/uf#1377).

import * as React from "@uniflowed/react";
import { useActionState, useState } from "@uniflowed/react";

import { add, type NoteState, saveNote } from "../_actions/counter.js";

/** What the note is before anything was submitted. */
const NO_NOTE: NoteState = { saved: null };

/** A JSON-path action button, and a progressively enhanced form. */
export default component Actions() {
  const [total, setTotal] = useState<string>("");
  const [note, submit] = useActionState<NoteState, FormData>(saveNote, NO_NOTE);
  return (
    <section>
      <button
        id="add"
        type="button"
        onClick={() => {
          void add(41).then((answer) => {
            setTotal(String(answer.total));
          });
        }}
      >
        add
      </button>
      <output id="total">{total}</output>
      <form id="note-form" action={submit}>
        <input id="note" name="note" defaultValue="" />
        <button id="save" type="submit">
          save
        </button>
        <output id="saved">{note.saved ?? ""}</output>
      </form>
    </section>
  );
}
