"use client";
// @flow
import * as React from "@uniflowed/react";
import { useActionState } from "@uniflowed/react";

import { type NoteState, saveNote } from "../_actions/notes.js";

const NOTHING_YET: NoteState = { saved: null, problem: null };

export component NoteForm() {
  const [state, save, pending] = useActionState<NoteState, FormData>(saveNote, NOTHING_YET);
  return (
    <form action={save}>
      <label>
        Note <input name="text" maxLength={80} />
      </label>
      <button type="submit" disabled={pending}>
        Save
      </button>
      {/* A status paragraph rather than <output>: React resets the form after
          the action, and happy-dom's reset empties an <output> where a browser
          keeps it (capricorn86/happy-dom#2441), which would blank this text in
          a component test. */}
      <p role="status">
        {state.problem ?? (state.saved == null ? "" : `saved note ${state.saved}`)}
      </p>
    </form>
  );
}
