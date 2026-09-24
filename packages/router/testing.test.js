// @flow
//
// `@uniflowed/router/testing`, held to the claim in its header: every helper
// is the real code path at its level. So each case asks the question a test
// author would get wrong if the helper took a shortcut — does an argument that
// cannot cross fail before the action runs, does a redirect through the JSON
// door come back as the `500` a browser gets, does `after()` run before the
// test asserts, does a mutation's invalidation reach the store that answered.
//
// The build-level half (`buildApp` over a real project) needs `uf build`, and
// is `crates/uf_cli/tests/fixtures/rsc-test-app`'s `app/notes/notes.test.js`,
// run by the RSC job in CI. Here `openBuild` is driven over a hand-written
// `handler.js`, which is enough to pin the order it answers in and when it
// settles.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { describe, expect, fn, it } from "@uniflowed/test";
import { after, cookies, draftMode, headers } from "@uniflowed/server";
import {
  OutsideCachedRequestError,
  createCacheStore,
  revalidatePath,
  revalidateTag,
} from "@uniflowed/server/cache";

import { ServerActionError } from "./action.js";
import { ActionValueError } from "./internal/action-wire.js";
import { NotFoundError, forbidden, notFound, redirect, unauthorized } from "./internal/routing.js";
import {
  TEST_ACTION_ID,
  TEST_ORIGIN,
  buildApp,
  callAction,
  formsIn,
  openBuild,
  serverReferences,
  testRequest,
  withRequest,
} from "./testing.js";

function form(entries: { readonly [string]: string }): FormData {
  const data = new FormData();
  for (const [name, value] of Object.entries(entries)) data.append(name, value);
  return data;
}

describe("testRequest", () => {
  it("addresses a path to the test origin, with that origin's Host", () => {
    const request = testRequest({ url: "/notes?page=2" });
    expect(request.url).toBe(`${TEST_ORIGIN}/notes?page=2`);
    expect(request.method).toBe("GET");
    expect(request.headers.get("host")).toBe("localhost");
  });

  it("writes cookies into the Cookie header, after one the headers already carried", () => {
    const request = testRequest({
      headers: { cookie: "theme=dark" },
      cookies: { session: "a b", visitor: "v1" },
    });
    expect(request.headers.get("cookie")).toBe("theme=dark; session=a%20b; visitor=v1");
  });
});

describe("withRequest", () => {
  it("answers cookies() and headers() about the request it describes", async () => {
    const seen = await withRequest(
      { cookies: { session: "s-1" }, headers: { "accept-language": "ja" } },
      () => [cookies().get("session"), headers().get("accept-language")],
    );
    expect(seen).toEqual(["s-1", "ja"]);
  });

  it("has run what after() deferred by the time it resolves", async () => {
    const ran = fn();
    await withRequest({}, async () => {
      after(() => ran("after"));
      expect(ran).not.toHaveBeenCalled();
    });
    expect(ran).toHaveBeenCalledWith("after");
  });

  it("passes a routing error through, so a test asserts on the error a page threw", async () => {
    await expect(withRequest({}, () => notFound())).rejects.toBeInstanceOf(NotFoundError);
  });

  it("keeps two interleaved requests apart", async () => {
    const who = async (name: string) =>
      withRequest({ cookies: { who: name } }, async () => {
        await new Promise((resolve) => setTimeout(resolve, name === "first" ? 5 : 0));
        return cookies().get("who");
      });
    expect(await Promise.all([who("first"), who("second")])).toEqual(["first", "second"]);
  });

  it("installs no cache unless asked, as a project without rendering.cache has none", async () => {
    await expect(withRequest({}, () => revalidateTag("notes"))).rejects.toBeInstanceOf(
      OutsideCachedRequestError,
    );
    const store = createCacheStore();
    expect(await withRequest({ cache: store }, () => revalidateTag("notes"))).toBe(0);
  });
});

describe("callAction through the fetch door", () => {
  it("runs the action inside the request and answers with what JSON carried back", async () => {
    const returned = { note: "hello", by: "" };
    const save = async (note: string) => {
      returned.by = cookies().get("session") ?? "";
      return returned;
    };
    const outcome = await callAction(save, ["hello"], { cookies: { session: "s-1" } });
    expect(outcome.kind).toBe("returned");
    if (outcome.kind !== "returned") return;
    // Equal, and not the same object: it was decoded from the answer.
    expect(outcome.value).toEqual({ note: "hello", by: "s-1" });
    expect(outcome.value).not.toBe(returned);
    expect(outcome.response.status).toBe(200);
    expect(outcome.response.headers.get("cache-control")).toBe("no-store");
  });

  it("refuses an argument that cannot cross at the call site, before anything runs", async () => {
    const save = fn(async (_when: mixed) => null);
    // $FlowFixMe[incompatible-type] a Date is not an ActionArgument; that is the case under test.
    await expect(callAction(save, [new Date(0)])).rejects.toBeInstanceOf(ActionValueError);
    expect(save).not.toHaveBeenCalled();
  });

  it("names what the action threw, while the response is the fixed 500 a browser gets", async () => {
    const boom = new Error("database is down");
    const outcome = await callAction(async (): Promise<null> => {
      throw boom;
    }, []);
    expect(outcome.kind).toBe("threw");
    if (outcome.kind !== "threw") return;
    expect(outcome.error).toBe(boom);
    expect(outcome.response.status).toBe(500);
    expect(await outcome.response.text()).not.toContain("database");
  });

  it("tells a redirect from a failure, and shows the 204 and Location the fetch door answers with", async () => {
    const outcome = await callAction(async () => redirect("/notes"), []);
    expect(outcome.kind).toBe("redirect");
    if (outcome.kind !== "redirect") return;
    expect(outcome.to).toBe("/notes");
    expect(outcome.permanent).toBe(false);
    expect(outcome.response.status).toBe(204);
    expect(outcome.response.headers.get("location")).toBe("/notes");
  });

  it("names notFound(), unauthorized() and forbidden()", async () => {
    const kinds = await Promise.all([
      callAction(async () => notFound(), []),
      callAction(async () => unauthorized(), []),
      callAction(async () => forbidden(), []),
    ]);
    expect(kinds.map((outcome) => outcome.kind)).toEqual([
      "not-found",
      "unauthorized",
      "forbidden",
    ]);
  });

  it("says when a result cannot cross back, and why", async () => {
    const outcome = await callAction(async () => ({ saved: true, at: undefined }), []);
    expect(outcome.kind).toBe("unsendable");
    if (outcome.kind !== "unsendable") return;
    expect(outcome.error).toBeInstanceOf(ActionValueError);
    expect(outcome.error.path).toBe("the result.at");
    expect(outcome.response.status).toBe(500);
  });

  it("is refused, without running, when the test overrides a guard the endpoint keeps", async () => {
    const save = fn(async () => null);
    const outcome = await callAction(save, [], {
      headers: { origin: "https://elsewhere.example" },
    });
    expect(outcome.kind).toBe("refused");
    expect(outcome.response.status).toBe(403);
    expect(save).not.toHaveBeenCalled();
  });

  it("records what the action invalidated, and leaves the store as it found it", async () => {
    const store = createCacheStore();
    const outcome = await callAction(
      async () => {
        revalidateTag("notes");
        revalidatePath("/notes");
        return null;
      },
      [],
      { cache: store },
    );
    expect(outcome.revalidated).toEqual({ tags: ["notes"], paths: ["/notes"] });
    expect(Object.hasOwn(store, "revalidateTag")).toBe(false);
  });

  it("has settled the request, so after() has run", async () => {
    const ran = fn();
    await callAction(async () => {
      after(() => ran());
      return null;
    }, []);
    expect(ran).toHaveBeenCalledTimes(1);
  });

  it("puts the draft-mode cookie an action decided on onto its response", async () => {
    const outcome = await callAction(async () => {
      draftMode().enable();
      return null;
    }, []);
    expect(outcome.response.headers.get("set-cookie") ?? "").toContain("__Host-uf.draft=");
  });

  it("hands a form action the FormData the form submitted", async () => {
    const outcome = await callAction(
      async (previous: null, submitted: FormData) => [previous, submitted.get("note")],
      [null, form({ note: "hi" })],
    );
    expect(outcome.kind === "returned" ? outcome.value : null).toEqual([null, "hi"]);
  });
});

describe("callAction through the form door", () => {
  it("posts the form a browser posts before hydration, and answers with a 303 back", async () => {
    const outcome = await callAction(
      async (previous: {| readonly count: number |}, submitted: FormData) => ({
        count: previous.count + Number(submitted.get("step")),
      }),
      [{ count: 1 }, form({ step: "2" })],
      { door: "form", url: "/counter?tab=2" },
    );
    expect(outcome.kind).toBe("returned");
    if (outcome.kind !== "returned") return;
    expect(outcome.value).toEqual({ count: 3 });
    expect(outcome.response.status).toBe(303);
    expect(outcome.response.headers.get("location")).toBe("/counter?tab=2");
  });

  it("follows a redirect() with a 303 to where it pointed", async () => {
    const outcome = await callAction(async (_: FormData) => redirect("/notes/7"), [form({})], {
      door: "form",
    });
    expect(outcome.kind).toBe("redirect");
    expect(outcome.response.status).toBe(303);
    expect(outcome.response.headers.get("location")).toBe("/notes/7");
  });

  it("needs the form last", async () => {
    await expect(callAction(async (_: string) => null, ["x"], { door: "form" })).rejects.toThrow(
      "ends with the FormData",
    );
  });
});

describe("serverReferences", () => {
  const module = {
    greet: async (name: string) => `hello ${name}, ${cookies().get("session") ?? "guest"}`,
    fail: async () => {
      throw new Error("no");
    },
    LIMIT: 80,
  };

  it("calls each export through the wire, inside a request with the test's cookies", async () => {
    let session = "s-1";
    const references = serverReferences(module, () => ({ cookies: { session } }));
    expect(await references.greet("Ada")).toBe("hello Ada, s-1");
    session = "s-2";
    expect(await references.greet("Ada")).toBe("hello Ada, s-2");
    expect(references.LIMIT).toBe(80);
  });

  it("rejects the way a browser's reference does when the endpoint did not answer 200", async () => {
    const references = serverReferences(module, { name: "app/_actions/greeting.js#fail" });
    const failure = references.fail().catch((error: mixed) => error);
    expect(await failure).toBeInstanceOf(ServerActionError);
    expect((await failure).status).toBe(500);
  });

  it("gives a form the same $$FORM_ACTION the browser's reference has", () => {
    const references = serverReferences(module);
    // $FlowFixMe[prop-missing] React's own property, which Flow does not know.
    const fields = references.greet.$$FORM_ACTION("0");
    expect(fields.data.get("$uf_id_0")).toBe(TEST_ACTION_ID);
  });
});

describe("formsIn", () => {
  it("reads the hidden fields React writes for an action, entities decoded", () => {
    const html =
      '<form action="" encType="application/x-www-form-urlencoded" method="POST">' +
      '<input type="hidden" name="$uf_ref_0"/>' +
      '<input type="hidden" name="$uf_bound_0" value="{&quot;args&quot;:[null]}"/>' +
      '<input name="note" value="typed"/></form><form action="/b"></form>';
    expect(formsIn(html)).toEqual([
      {
        action: "",
        hidden: [
          ["$uf_ref_0", ""],
          ["$uf_bound_0", '{"args":[null]}'],
        ],
      },
      { action: "/b", hidden: [] },
    ]);
  });
});

describe("openBuild", () => {
  /** A directory shaped like `.uf/deploy/node`, with a handler that says what it saw. */
  function fakeBuild(): string {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-router-testing-"));
    fs.mkdirSync(path.join(root, "static"));
    fs.writeFileSync(path.join(root, "static", "robots.txt"), "User-agent: *\n");
    fs.writeFileSync(
      path.join(root, "handler.js"),
      [
        "export const settled = [];",
        "export function beginRequest(request) {",
        "  return { run: (body) => body(), settle: async () => { settled.push(new URL(request.url).pathname); } };",
        "}",
        "export async function fetch(request) {",
        "  const url = new URL(request.url);",
        '  if (request.method === "POST") return new Response(await request.text(), { status: 303, headers: { location: "/done", "x-origin": request.headers.get("origin") } });',
        '  if (url.pathname === "/notes/__uf.flight") return new Response("0:{}", { headers: { "content-type": "text/x-component" } });',
        '  if (url.pathname === "/form") return new Response(\'<form action=""><input type="hidden" name="$uf_id_0" value="abc"/></form>\', { headers: { "content-type": "text/html" } });',
        '  return new Response(`${request.headers.get("accept")} ${url.pathname}`);',
        "}",
      ].join("\n"),
    );
    return root;
  }

  it("answers files first, then the application, and settles once the body is read", async () => {
    const root = fakeBuild();
    try {
      const app = await openBuild(root);
      // $FlowFixMe[unsupported-syntax] the scratch build's handler, by its run-time path.
      const handler: $FlowFixMe = await import(path.join(root, "handler.js"));
      expect(await (await app.fetch("/robots.txt")).text()).toBe("User-agent: *\n");
      const page = await app.render("/notes");
      expect(handler.settled).not.toContain("/notes");
      expect(await page.text()).toBe("text/html /notes");
      await app.settled();
      expect(handler.settled).toContain("/notes");
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  it("asks for a route's Flight payload where a navigation does, and refuses anything else", async () => {
    const root = fakeBuild();
    try {
      const app = await openBuild(root);
      expect(await (await app.flight("/notes")).text()).toBe("0:{}");
      await expect(app.flight("/other")).rejects.toThrow("without a Flight payload");
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  it("submits a page's form with its hidden fields, from the page's own origin", async () => {
    const root = fakeBuild();
    try {
      const app = await openBuild(root);
      const answer = await app.submit("/form", { note: "hi" });
      expect(answer.status).toBe(303);
      expect(answer.headers.get("x-origin")).toBe(TEST_ORIGIN);
      expect(await answer.text()).toBe("%24uf_id_0=abc&note=hi");
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  it("refuses a directory that holds no handler, naming the one to open", async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-router-testing-"));
    try {
      fs.writeFileSync(path.join(root, "handler.js"), "export default 1;\n");
      await expect(openBuild(root)).rejects.toThrow(".uf/deploy/node");
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
});

describe("buildApp", () => {
  it("rejects with the build's own output when the build fails", async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-router-testing-"));
    const binary = path.join(root, "fake-uf.cjs");
    try {
      fs.writeFileSync(
        binary,
        '#!/usr/bin/env node\nconsole.error("error[rsc/server-only-import-in-client]: no");\nprocess.exit(1);\n',
      );
      fs.chmodSync(binary, 0o755);
      await expect(buildApp({ root, binary })).rejects.toThrow("rsc/server-only-import-in-client");
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
});
