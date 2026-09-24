"use server";
// @flow
import { cookies } from "@uniflowed/server";
import { revalidateTag } from "@uniflowed/server/cache";
import { forbidden, notFound } from "@uniflowed/router";
import { maxLength, minLength, object, pipe, safeParse, string, trim } from "@uniflowed/validator";

import { findNote, insertNote, removeNote } from "../_data/notes.server.js";

/** What `useActionState` holds between submits. */
export type NoteState = {| readonly saved: string | null, readonly problem: string | null |};

const NoteInput = object({ text: pipe(string(), trim(), minLength(1), maxLength(80)) });

/** Save a note for whoever is signed in. A refusal is a state, not an exception. */
export async function saveNote(previous: NoteState, form: FormData): Promise<NoteState> {
  // The action authorizes itself: a guard on the page's path is not a boundary.
  const author = cookies().get("session");
  if (author == null) {
    return { saved: previous.saved, problem: "sign in to write a note" };
  }
  const parsed = safeParse(NoteInput, { text: form.get("text") });
  if (!parsed.ok) {
    return { saved: previous.saved, problem: "a note is 1 to 80 characters" };
  }
  const note = await insertNote(parsed.value.text, author);
  revalidateTag("notes");
  return { saved: note.id, problem: null };
}

/** Delete a note. Only its author may; anyone else is refused outright. */
export async function deleteNote(id: string): Promise<void> {
  const note = await findNote(id);
  if (note == null) {
    throw notFound();
  }
  if (note.author !== cookies().get("session")) {
    throw forbidden();
  }
  await removeNote(note.id);
  revalidateTag("notes");
}
