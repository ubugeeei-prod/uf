// @flow
//
// Where the notes live: in this process's memory.
//
// It stands in for the service a real application would call — uf's server is
// a BFF, and the notes belong to whoever owns them. `.server.js` keeps it out
// of every client graph: importing it from a `"use client"` module is a build
// error, not a leak.
import { cacheFunction } from "@uniflowed/server/cache";

export type Note = {| readonly id: string, readonly text: string, readonly author: string |};

let notes: $ReadOnlyArray<Note> = [
  { id: "1", text: "Server Components fetch their own data", author: "ada" },
];

/** Every note, cached under the `notes` tag when `app.rendering.cache.data` is on. */
export const listNotes: () => Promise<$ReadOnlyArray<Note>> = cacheFunction(
  "notes.list.v1",
  async () => notes,
  { lifetime: { revalidate: 300 }, tags: ["notes"] },
);

export async function findNote(id: string): Promise<Note | null> {
  return notes.find((note) => note.id === id) ?? null;
}

export async function insertNote(text: string, author: string): Promise<Note> {
  const note = { id: String(notes.length + 1), text, author };
  notes = [...notes, note];
  return note;
}

export async function removeNote(id: string): Promise<void> {
  notes = notes.filter((note) => note.id !== id);
}

/** Put the store back the way it started, so one test cannot see another's notes. */
export function resetNotes(): void {
  notes = [{ id: "1", text: "Server Components fetch their own data", author: "ada" }];
}
