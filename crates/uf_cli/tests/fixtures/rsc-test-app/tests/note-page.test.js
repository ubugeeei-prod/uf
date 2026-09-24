// @flow
import { describe, expect, it } from "@uniflowed/test";
import { NotFoundError } from "@uniflowed/router";
import { withRequest } from "@uniflowed/router/testing";

import { Page as NotePage } from "../app/notes/[id]/$page.js";

// The page's decisions, without rendering it: which note, missing or malformed.
// What the rendered answer looks like is `notes-app.test.js`, over a real build.
describe("the note page's decisions", () => {
  it("asks for the not-found boundary when the note does not exist", async () => {
    await expect(
      withRequest({ url: "/notes/404" }, () => NotePage({ params: { id: "404" } })),
    ).rejects.toBeInstanceOf(NotFoundError);
  });

  it("throws for a malformed id, which the error boundary answers", async () => {
    await expect(
      withRequest({ url: "/notes/x" }, () => NotePage({ params: { id: "x" } })),
    ).rejects.toThrow("note ids are numbers");
  });
});
