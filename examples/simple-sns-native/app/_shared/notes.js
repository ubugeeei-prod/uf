// @flow

import * as React from "react";

export type Note = {|
  readonly id: string,
  readonly author: string,
  readonly handle: string,
  readonly topic: string,
  readonly body: string,
|};

const initial: Array<Note> = [
  {
    id: "garden",
    author: "Mika Tanaka",
    handle: "mika",
    topic: "NEIGHBORHOOD",
    body: "The little garden behind the library is in bloom. A good excuse to take the long way home.",
  },
  {
    id: "coffee",
    author: "Ren Sato",
    handle: "ren",
    topic: "EVERYDAY",
    body: "An open window, a fresh notebook, and coffee that went cold while we talked. A lovely morning.",
  },
  {
    id: "walk",
    author: "Sora Kim",
    handle: "sora",
    topic: "OUTDOORS",
    body: "Saturday walk along the river, anyone? Meet by the footbridge at nine. Bring a friend.",
  },
];

type Notes = {| readonly notes: $ReadOnlyArray<Note>, readonly publish: (string) => void |};

const Context: React.Context<Notes | null> = React.createContext(null);

export component NotesProvider(children: React.Node) {
  const [notes, setNotes] = React.useState(initial);
  const publish = (body: string) => {
    const text = body.trim();
    if (text.length === 0) return;
    setNotes((current) => [
      { id: String(Date.now()), author: "You", handle: "you", topic: "EVERYDAY", body: text },
      ...current,
    ]);
  };
  return <Context.Provider value={{ notes, publish }}>{children}</Context.Provider>;
}

export function useNotes(): Notes {
  const notes = React.useContext(Context);
  if (notes == null) throw new Error("NotesProvider is missing");
  return notes;
}
