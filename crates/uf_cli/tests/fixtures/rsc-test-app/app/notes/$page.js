// @flow
import * as React from "@uniflowed/react";

import { NoteForm } from "./_components/NoteForm.js";
import { listNotes } from "./_data/notes.server.js";

// Rendered per request: the notes change while the server runs.
export const dynamic = "force-dynamic";

async function NoteCount() {
  const notes = await listNotes();
  return <p id="count">{`${notes.length} notes`}</p>;
}

export async function Page(): Promise<React.Node> {
  const notes = await listNotes();
  return (
    <main>
      <h1>Notes</h1>
      <ul id="notes">
        {notes.map((note) => (
          <li key={note.id}>
            <a href={`/notes/${note.id}`}>{note.text}</a>
          </li>
        ))}
      </ul>
      <React.Suspense fallback={<p id="count">counting</p>}>
        <NoteCount />
      </React.Suspense>
      <NoteForm />
    </main>
  );
}
