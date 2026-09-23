// @flow
import { beforeAll, describe, expect, it } from "@uniflowed/test";
import { type BuiltApp, buildApp } from "@uniflowed/router/testing";

let app: BuiltApp;

// One production build for the whole file: `uf build --adapter node`, then the
// handler it wrote, answering in this process.
beforeAll(
  async () => {
    app = await buildApp({ root: new URL("../", import.meta.url) });
  },
  { timeout: 120_000 },
);

describe("the notes route, built", () => {
  it("streams a document with the Suspense boundary resolved inside it", async () => {
    const response = await app.render("/notes");
    expect(response.status).toBe(200);
    expect(response.headers.get("content-type")).toContain("text/html");
    const html = await response.text();
    expect(html).toContain("Server Components fetch their own data");
    expect(html).toContain("1 notes");
  });

  it("sends a navigation the Flight payload, naming the client component, not its code", async () => {
    const payload = await (await app.flight("/notes")).text();
    expect(payload).toContain("Server Components fetch their own data");
    expect(payload).toContain('"NoteForm"');
    expect(payload).not.toContain("useActionState");
  });

  it("answers a note that does not exist with the nearest not-found page", async () => {
    const response = await app.render("/notes/404");
    expect(response.status).toBe(404);
    expect(await response.text()).toContain("No such note.");
  });

  it("saves a note from a form posted before hydration, and says so on the page", async () => {
    const page = await app.submit(
      "/notes",
      { text: "Built, then submitted" },
      {
        cookies: { session: "grace" },
      },
    );
    expect(page.status).toBe(200);
    expect(await page.text()).toContain("saved note 2");
  });

  it.skipBecause(
    "lists a note saved by the action on the next render",
    "#1469: the action and the page run two copies of notes.server.js, so the page never sees the write",
    async () => {
      await app.submit("/notes", { text: "Seen by the page" }, { cookies: { session: "grace" } });
      expect(await (await app.render("/notes")).text()).toContain("Seen by the page");
    },
  );

  it("refuses a note from nobody, as state the page renders", async () => {
    const page = await (await app.submit("/notes", { text: "anonymous" })).text();
    expect(page).toContain("sign in to write a note");
  });

  it("answers a page that throws with the error boundary, a 500 and none of the message", async () => {
    const response = await app.render("/notes/not-a-number");
    expect(response.status).toBe(500);
    const html = await response.text();
    expect(html).toContain("This note did not load.");
    expect(html).not.toContain("note ids are numbers");
  });

  it("renders an existing note's page", async () => {
    expect(await (await app.render("/notes/1")).text()).toContain("by ada");
  });
});
