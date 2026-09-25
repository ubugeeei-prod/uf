// @flow
//
// A route handler writing to the store the notes page reads.
//
// The handler and the page must run one instance of `notes.server.js`: a
// handler loaded in a graph of its own wrote to a copy, and the page never saw
// the note (ubugeeei-prod/uf#1487).

import { revalidateTag } from "@uniflowed/server/cache";

import { insertNote } from "../../notes/_data/notes.server.js";

export async function POST(request: Request): Promise<Response> {
  const form = await request.formData();
  const text = form.get("text");
  if (typeof text !== "string" || text.trim() === "") {
    return Response.json({ problem: "a note needs text" }, { status: 400 });
  }
  const note = await insertNote(text.trim(), "api");
  revalidateTag("notes");
  return Response.json({ id: note.id }, { status: 201 });
}
