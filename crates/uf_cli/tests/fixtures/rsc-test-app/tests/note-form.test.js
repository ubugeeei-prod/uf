// @flow
import * as React from "@uniflowed/react";
import typeof * as NoteActions from "../app/notes/_actions/notes.js";
import { afterEach, describe, expect, it, uft } from "@uniflowed/test";
import { cleanup, render, screen, userEvent, waitFor } from "@uniflowed/react-testing";
import { createCacheStore } from "@uniflowed/server/cache";
import { serverReferences } from "@uniflowed/router/testing";

const ACTIONS = "../app/notes/_actions/notes.js";

/** The form, with its actions reached through the wire as `session` would reach them. */
async function renderForm(session: string | null) {
  await uft.mock<NoteActions>(ACTIONS, async () =>
    serverReferences(await uft.importActual<NoteActions>(ACTIONS), {
      url: "/notes",
      cookies: session == null ? {} : { session },
      // `app.rendering.cache` is on in this project, so the host installs a store.
      cache: createCacheStore(),
    }),
  );
  const { NoteForm } = await import("../app/notes/_components/NoteForm.js");
  render(<NoteForm />);
}

afterEach(() => {
  cleanup();
  uft.unmock(ACTIONS);
});

describe("NoteForm", () => {
  it("saves through the real action and shows what it answered", async () => {
    await renderForm("grace");
    await userEvent.type(screen.getByLabelText("Note"), "Tested through the wire");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.getByRole("status").textContent).toBe("saved note 2"));
    // The second save is note 3: the first one is in the store the action wrote to.
    await userEvent.type(screen.getByLabelText("Note"), "And again");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.getByRole("status").textContent).toBe("saved note 3"));
  });

  it("shows the action's refusal when nobody is signed in", async () => {
    await renderForm(null);
    await userEvent.type(screen.getByLabelText("Note"), "Anonymous");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toBe("sign in to write a note"),
    );
  });
});
