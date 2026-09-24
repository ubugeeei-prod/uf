// @flow
import * as React from "@uniflowed/react";
import { notFound } from "@uniflowed/router";

import { findNote } from "../_data/notes.server.js";

export async function Page({
  params,
}: {
  readonly params: {| readonly id: string |},
  ...
}) {
  // A malformed id is a bug in whatever linked here, not a missing note.
  if (!/^[0-9]+$/.test(params.id)) {
    throw new Error(`note ids are numbers, not ${JSON.stringify(params.id)}`);
  }
  const note = await findNote(params.id);
  if (note == null) {
    throw notFound();
  }
  return (
    <article>
      <h1>{note.text}</h1>
      <p>{`by ${note.author}`}</p>
    </article>
  );
}
