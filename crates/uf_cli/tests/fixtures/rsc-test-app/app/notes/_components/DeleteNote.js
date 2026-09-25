"use client";
// @flow
//
// A button that calls a server action it was handed as a prop.
//
// The note page, a Server Component, passes `deleteNote.bind(null, note.id)`:
// Flight sends it as a server reference with the id bound, and the browser
// gets a function that calls the action over uf's action wire with the id
// first (ubugeeei-prod/uf#1359). Nothing here imports the action module.
import * as React from "@uniflowed/react";
import { useTransition } from "@uniflowed/react";

export component DeleteNote(remove: () => Promise<void>) {
  const [pending, startTransition] = useTransition();
  return (
    <button
      type="button"
      disabled={pending}
      onClick={() => {
        startTransition(async () => {
          await remove();
        });
      }}>
      Delete
    </button>
  );
}
