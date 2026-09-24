// @flow
"use server";

// The server actions the matrix calls, both ways a browser can.
//
// `add` is called the way a hydrated client reference calls it: a `POST` with
// `uf-action` and a JSON envelope. `saveNote` is bound to a `<form>` through
// `useActionState`, so before hydration the form is a real HTML form that posts
// `application/x-www-form-urlencoded` fields (ubugeeei-prod/uf#1377) and the
// answer is the page rendered again with the action's state in it.
//
// Both answers are computed from their input, so a check can tell the
// action's answer from anything else a host might return with a `200`.

import { cookies } from "@uniflowed/server";

/** Add one to `count`, and say who asked (the `visitor` cookie). */
export async function add(count: number): Promise<{|
  readonly total: number,
  readonly visitor: string,
|}> {
  return { total: count + 1, visitor: cookies().get("visitor") ?? "anonymous" };
}

/** What `useActionState` holds between submits. */
export type NoteState = {| readonly saved: string | null |};

/** Save the submitted `note`, answering with it reversed so the result is the server's. */
export async function saveNote(previous: NoteState, form: FormData): Promise<NoteState> {
  const note = form.get("note");
  if (typeof note !== "string" || note === "") {
    return previous;
  }
  return { saved: `saved: ${Array.from(note).reverse().join("")}` };
}
