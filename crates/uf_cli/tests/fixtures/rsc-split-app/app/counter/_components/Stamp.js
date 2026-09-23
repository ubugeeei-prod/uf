"use client";
// @flow
//
// A form whose action a Server Component handed down as a prop.
//
// `Counter.js` imports its actions; this component imports none. The page — a
// Server Component — passes `submitNote` in, and Flight carries it across the
// boundary as a server reference: an id, and nothing of the module behind it.
// In the browser the prop is a function that calls through the same endpoint
// an imported action does; in the prerendered document it is a form that posts
// before hydration. See ubugeeei-prod/uf#1359.

import * as React from "@uniflowed/react";
import { useActionState } from "@uniflowed/react";

import type { NoteState } from "../_actions/tally.js";

const NO_NOTE: NoteState = { saved: null, problem: null };

export default component Stamp(note: (previous: NoteState, form: FormData) => Promise<NoteState>) {
  const [stamped, stamp] = useActionState<NoteState, FormData>(note, NO_NOTE);
  return (
    <form action={stamp} data-stamp="true">
      <input name="note" aria-label="stamp" defaultValue="" />
      <button type="submit">stamp</button>
      <output data-stamped="true">{stamped.saved ?? stamped.problem ?? ""}</output>
    </form>
  );
}
