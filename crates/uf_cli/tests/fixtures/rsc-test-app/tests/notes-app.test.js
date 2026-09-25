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

  it("answers a payload asked for as not found with that page, for a URL that exists", async () => {
    // What a page asks for when a hydrated action called `notFound()` (#1489):
    // the not-found page a loader's `notFound()` would have shown, and a 404.
    const response = await app.flight("/notes/1", { headers: { "uf-not-found": "1" } });
    expect(response.status).toBe(404);
    const payload = await response.text();
    expect(payload).toContain("No such note.");
    expect(payload).not.toContain("by ada");
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

  it("lists a note saved by the action on the next render", async () => {
    // The action and the page share one instance of `notes.server.js` (#1469).
    await app.submit("/notes", { text: "Seen by the page" }, { cookies: { session: "grace" } });
    expect(await (await app.render("/notes")).text()).toContain("Seen by the page");
  });

  it("lists a note a route handler saved on the next render", async () => {
    // The handler and the page share one instance of `notes.server.js` (#1487).
    const answer = await app.fetch("/api/notes", {
      method: "POST",
      body: new URLSearchParams({ text: "Posted to the handler" }),
    });
    expect(answer.status).toBe(201);
    expect(await (await app.render("/notes")).text()).toContain("Posted to the handler");
  });

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

/** A server reference as a payload writes it: the reference's id and what is bound to it. */
type SentReference = {| readonly id: string, readonly bound: $ReadOnlyArray<mixed> |};

/**
 * The server reference to `name` in a Flight payload, read off its rows.
 *
 * React writes one as an outlined row, `{"id":…,"bound":"$@<row>"}`, with the
 * bound arguments in the row it points at. Read here the way the browser's
 * Flight client reads it, so the test can make the call that client would.
 */
function sentReference(payload: string, name: string): SentReference {
  const rows: Map<string, string> = new Map();
  for (const line of payload.split("\n")) {
    const colon = line.indexOf(":");
    if (colon > 0) rows.set(line.slice(0, colon), line.slice(colon + 1));
  }
  for (const row of rows.values()) {
    if (!row.startsWith("{") || !row.includes(`#${name}"`)) continue;
    const reference: { readonly id?: mixed, readonly bound?: mixed, ... } = JSON.parse(row);
    const { id, bound } = reference;
    if (typeof id !== "string") continue;
    const boundRow = typeof bound === "string" ? rows.get(bound.slice(2)) : null;
    const boundArguments: $ReadOnlyArray<mixed> = boundRow == null ? [] : JSON.parse(boundRow);
    return { id, bound: boundArguments };
  }
  throw new Error(`no server reference to ${name} in the payload:\n${payload}`);
}

describe("a server action a Server Component hands to a Client Component (#1359)", () => {
  it("crosses as a server reference under the action's id, with the note's id bound", async () => {
    const payload = await (await app.flight("/notes/1")).text();
    const reference = sentReference(payload, "app/notes/_actions/notes.js#deleteNote");
    expect(reference.id).toMatch(/^[0-9a-f]{64}#app\/notes\/_actions\/notes\.js#deleteNote$/);
    expect(reference.bound).toEqual(["1"]);
    // The HTML renderer decodes the same payload and renders the button.
    expect(await (await app.render("/notes/1")).text()).toContain("Delete</button>");
  });

  it("reaches the action over the JSON wire, with the bound id first", async () => {
    const payload = await (await app.flight("/notes/1")).text();
    const { id, bound } = sentReference(payload, "app/notes/_actions/notes.js#deleteNote");
    // What the reference the browser decodes sends: the action's id in
    // `uf-action`, and the bound arguments ahead of the call's own.
    const answer = await app.fetch("/notes/1", {
      method: "POST",
      headers: {
        origin: "http://localhost",
        "content-type": "application/json",
        "uf-action": id.slice(0, id.indexOf("#")),
      },
      cookies: { session: "mallory" },
      body: JSON.stringify({ args: bound }),
    });
    // The action found note 1 from the bound id and refused mallory, who did
    // not write it: `forbidden()`, not the `notFound()` a missing id would be.
    expect(answer.status).toBe(403);
    expect(answer.headers.get("uf-action-outcome")).toBe("forbidden");
  });

  it("still refuses a function that is not a server action", async () => {
    // Flight writes an error row where the prop would have been: the function
    // is not sent, and nothing can call it.
    const payload = await (await app.flight("/claims/plain-function")).text();
    const row = /"remove":"\$([0-9a-f]+)"/.exec(payload)?.[1];
    expect(row).not.toBe(undefined);
    expect(payload).toContain(`\n${row ?? ""}:E{`);
    // And the document is the route's error boundary.
    const response = await app.render("/claims/plain-function");
    expect(response.status).toBe(500);
    expect(await response.text()).toContain("This page could not be rendered.");
  });
});
