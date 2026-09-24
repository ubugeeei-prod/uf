// @flow
import { beforeEach, describe, expect, it } from "@uniflowed/test";
import { createCacheStore } from "@uniflowed/server/cache";
import { callAction } from "@uniflowed/router/testing";

import { deleteNote, saveNote } from "../app/notes/_actions/notes.js";
import { listNotes, resetNotes } from "../app/notes/_data/notes.server.js";

const NOTHING_YET = { saved: null, problem: null };

function note(text: string): FormData {
  const form = new FormData();
  form.set("text", text);
  return form;
}

beforeEach(() => resetNotes());

describe("saveNote", () => {
  it("saves a note for the signed-in author and invalidates the list", async () => {
    const outcome = await callAction(saveNote, [NOTHING_YET, note("  Flight is a wire  ")], {
      cookies: { session: "grace" },
      cache: createCacheStore(),
    });
    const saved = match (outcome) {
      {kind: "returned", const value, ...} => value,
      _ => outcome.kind,
    };
    expect(saved).toEqual({ saved: "2", problem: null });
    expect(outcome.revalidated.tags).toEqual(["notes"]);
    expect((await listNotes()).map((saved) => saved.text)).toContain("Flight is a wire");
  });

  it("answers a validation problem as state, keeping what was saved before", async () => {
    const outcome = await callAction(saveNote, [{ saved: "1", problem: null }, note("   ")], {
      cookies: { session: "grace" },
    });
    expect(outcome.kind === "returned" ? outcome.value : null).toEqual({
      saved: "1",
      problem: "a note is 1 to 80 characters",
    });
  });

  it("authorizes itself, whatever page the call came from", async () => {
    const outcome = await callAction(saveNote, [NOTHING_YET, note("hello")], { url: "/notes" });
    expect(outcome.kind === "returned" ? outcome.value.problem : null).toBe(
      "sign in to write a note",
    );
    expect(await listNotes()).toHaveLength(1);
  });
});

describe("deleteNote", () => {
  it("lets the author delete their note", async () => {
    const outcome = await callAction(deleteNote, ["1"], {
      cookies: { session: "ada" },
      cache: createCacheStore(),
    });
    expect(outcome.kind).toBe("returned");
    expect(await listNotes()).toEqual([]);
  });

  it("refuses anyone else with a 403, which the page's error boundary shows", async () => {
    const outcome = await callAction(deleteNote, ["1"], { cookies: { session: "mallory" } });
    expect(outcome.kind).toBe("forbidden");
    expect(outcome.response.status).toBe(403);
    expect(await listNotes()).toHaveLength(1);
  });

  it("says a missing note is not found", async () => {
    const outcome = await callAction(deleteNote, ["404"], { cookies: { session: "ada" } });
    expect(outcome.kind).toBe("not-found");
  });
});
