// @flow
//
// A form bound to a server action, before its page has hydrated.
//
// React 19's progressive enhancement is a contract between three parties, and
// each has a section here:
//
// 1. **The function.** React asks it for `$$FORM_ACTION` while rendering to
//    HTML and writes what it answers into the markup. `registerServerAction`
//    (the server's copy) and `createServerReference` (the browser's) must both
//    answer, and answer the same.
// 2. **The render.** Driven through `createRenderer`, the renderer every host
//    uses, so what is asserted is the document a person without JavaScript
//    would get — and, for a postback, the document that puts the action's
//    result back into the `useActionState` that submitted.
// 3. **The endpoint.** `createActionDispatcher`'s second door: the native post,
//    every refusal it makes, and the requests it must leave alone because they
//    are somebody else's.
//
// The end-to-end half — the bundler giving a real `"use server"` module the
// property — is `tests/library/server-actions.test.js`, beside the rest of what
// the bundler does with those modules.

import * as React from "@uniflowed/react";
import { useActionState } from "@uniflowed/react";
import { routerView } from "@uniflowed/router";
import { createServerReference, registerServerAction } from "@uniflowed/router/action";
import { beginRequest, createActionDispatcher, createRenderer } from "@uniflowed/router/server";
import { draftMode } from "@uniflowed/server";
import { describe, expect, it } from "@uniflowed/test";

import { installDom } from "../../packages/react-testing/internal/dom.js";
import { encodeActionArguments } from "./internal/action-wire.js";
import {
  FORM_ACTION_CONTENT_TYPE,
  type FormState,
  formStateScript,
  readFormPost,
  readFormState,
} from "./internal/form-action.js";
import { RedirectError } from "./internal/routing.js";

const ID = "a1b2c3d4e5f6".repeat(6).slice(0, 64);
const OTHER = "9f8e7d6c5b4a".repeat(6).slice(0, 64);
const assets = { scripts: ["/assets/client.js"], styles: [], preloads: [] };

/** What a `useActionState` form holds between submits. */
type Note = {| readonly saved: string | null |};

/** Every `name="…" value="…"` hidden field in `html`, in order. */
function hiddenFields(html: string): Array<[string, string]> {
  const fields: Array<[string, string]> = [];
  const pattern = /<input type="hidden" name="([^"]*)"(?: value="([^"]*)")?\/>/g;
  let found = pattern.exec(html);
  while (found != null) {
    fields.push([decode(found[1]), decode(found[2] ?? "")]);
    found = pattern.exec(html);
  }
  return fields;
}

/** Undo the attribute escaping React writes. */
function decode(text: string): string {
  return text
    .replace(/&quot;/g, '"')
    .replace(/&#x27;/g, "'")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&");
}

/** A one-route table whose page is `Page`. */
function tableOf(Page: React.ComponentType<{}>) {
  return {
    routes: [
      {
        path: "/notes",
        params: [],
        mdx: false,
        file: "app/notes/$page.js",
        page: () => Promise.resolve({ default: Page }),
        layouts: [],
        loading: [],
      },
    ],
    notFound: [],
    errors: [],
  };
}

/** Render `/notes` the way a host does, optionally as a postback. */
async function documentOf(Page: React.ComponentType<{}>, formState?: FormState): Promise<string> {
  const renderer = createRenderer({ App: routerView("./app"), ...tableOf(Page) });
  const lifecycle = beginRequest(new Request("http://localhost/notes"));
  try {
    return await lifecycle.run(async () => {
      const result = await renderer.render("/notes", assets, { formState });
      return await result.text();
    });
  } finally {
    await lifecycle.settle();
  }
}

// ---------------------------------------------------------------------------
// The function
// ---------------------------------------------------------------------------

describe("what a server action tells React about its form", () => {
  it("answers with a POST, the one content type, and the action's id", () => {
    const action: $FlowFixMe = registerServerAction(async () => undefined, ID);
    const fields = action.$$FORM_ACTION("R0");
    expect(fields.method).toBe("POST");
    expect(fields.encType).toBe(FORM_ACTION_CONTENT_TYPE);
    expect(fields.name).toBe("$uf_ref_R0");
    expect([...fields.data.entries()]).toEqual([["$uf_id_R0", ID]]);
  });

  it("answers the same from the browser's reference as from the server's function", () => {
    const reference: $FlowFixMe = createServerReference(ID, "app/_actions/notes.js#save");
    const server: $FlowFixMe = registerServerAction(async () => undefined, ID);
    const a = reference.$$FORM_ACTION("x");
    const b = server.$$FORM_ACTION("x");
    expect([a.name, a.method, a.encType, [...a.data.entries()]]).toEqual([
      b.name,
      b.method,
      b.encType,
      [...b.data.entries()],
    ]);
  });

  it("carries bound arguments in the form, and still binds", async () => {
    const seen = [];
    const action: $FlowFixMe = registerServerAction(async (...args: Array<mixed>) => {
      seen.push(args);
      return "done";
    }, ID);
    const bound = action.bind(null, { saved: "draft" });

    // Still the function it was: calling the bound one on the server calls
    // the action with the bound argument first.
    expect(await bound("more")).toBe("done");
    expect(seen).toEqual([[{ saved: "draft" }, "more"]]);

    const fields = bound.$$FORM_ACTION("f");
    expect(fields.data.get("$uf_bound_f")).toBe(encodeActionArguments([{ saved: "draft" }]));
    // React asks the unbound action whether a postback is its own, by id and
    // by how many times it was bound besides the state.
    expect(action.$$IS_SIGNATURE_EQUAL(ID, 0)).toBe(true);
    expect(action.$$IS_SIGNATURE_EQUAL(OTHER, 0)).toBe(false);
    expect(bound.$$IS_SIGNATURE_EQUAL(ID, 0)).toBe(false);
    expect(bound.$$IS_SIGNATURE_EQUAL(ID, 1)).toBe(true);
  });

  it("refuses to write a bound argument the wire could not carry", () => {
    const action: $FlowFixMe = registerServerAction(async () => undefined, ID);
    // React catches this, logs "Failed to serialize an action for progressive
    // enhancement", and writes the form it writes for a client action.
    expect(() => action.bind(null, new Date(0)).$$FORM_ACTION("p")).toThrow("class instance");
  });

  it("leaves a value that is not a function alone", () => {
    const value = { not: "a function" };
    expect(registerServerAction(value, ID)).toBe(value);
  });
});

// ---------------------------------------------------------------------------
// The render
// ---------------------------------------------------------------------------

describe("a page with a form bound to a server action, before hydration", () => {
  it("is a real form: POST to the page, urlencoded, with the action named", async () => {
    const save = registerServerAction(async (form: FormData) => String(form.get("note")), ID);
    component Page() {
      return (
        <form action={save}>
          <input name="note" aria-label="note" />
          <button type="submit">save</button>
        </form>
      );
    }
    const html = await documentOf(Page);

    expect(html).not.toContain("javascript:");
    expect(html).toContain('method="POST"');
    // React spells the attribute `encType`; HTML attribute names are not case-sensitive.
    expect(html).toContain(`encType="${FORM_ACTION_CONTENT_TYPE}"`);
    expect(html).toContain(`action=""`);
    const fields = hiddenFields(html);
    const ref = fields.find(([name]) => name.startsWith("$uf_ref_"));
    expect(ref != null).toBe(true);
    const prefix = (ref?.[0] ?? "").slice("$uf_ref_".length);
    expect(fields).toContainEqual([`$uf_id_${prefix}`, ID]);
  });

  it("is what a form bound to a plain function is not", async () => {
    // The contrast that says the property is what made the difference: React
    // writes this for any function it cannot post natively.
    const local = async (_form: FormData) => undefined;
    component Page() {
      return (
        <form action={local}>
          <button type="submit">save</button>
        </form>
      );
    }
    expect(await documentOf(Page)).toContain("javascript:throw");
  });

  it("carries a useActionState's state and key, so the answer can find it again", async () => {
    const save = registerServerAction(
      async (previous: Note, form: FormData): Promise<Note> => ({
        saved: String(form.get("note")),
      }),
      ID,
    );
    component Page() {
      const [note, submit] = useActionState<Note, FormData>(save, { saved: null });
      return (
        <form action={submit}>
          <input name="note" aria-label="note" />
          <output>{note.saved ?? "nothing saved"}</output>
        </form>
      );
    }
    const html = await documentOf(Page);
    const fields = new Map(hiddenFields(html));
    const prefix = [...fields.keys()].find((name) => name.startsWith("$uf_ref_"))?.slice(8) ?? "";
    expect(fields.get(`$uf_bound_${prefix}`)).toBe(encodeActionArguments([{ saved: null }]));
    expect(typeof fields.get("$ACTION_KEY")).toBe("string");
    expect(html).toContain("nothing saved");
    // An ordinary render carries no form state.
    expect(html).not.toContain("uf:form-state");
  });

  it("answers a postback with the action's result in the hook that submitted", async () => {
    const save = registerServerAction(
      async (previous: Note, form: FormData): Promise<Note> => ({
        saved: String(form.get("note")),
      }),
      ID,
    );
    component Page() {
      const [note, submit] = useActionState<Note, FormData>(save, { saved: null });
      return (
        <form action={submit}>
          <input name="note" aria-label="note" />
          <output>{note.saved ?? "nothing saved"}</output>
        </form>
      );
    }
    const key = new Map(hiddenFields(await documentOf(Page))).get("$ACTION_KEY") ?? "";
    const formState: FormState = [{ saved: "hello" }, key, ID, 0];

    const html = await documentOf(Page, formState);
    expect(html).toContain("<output>hello</output>");
    // React marks the hook that took the state, which is how the browser's
    // hydration picks the same one…
    expect(html).toContain("<!--F!-->");
    // …and the shell carries the state for `hydrateRoot`, as data.
    expect(html).toContain(formStateScript(formState));
  });

  it("does not hand the state to a different action's hook", async () => {
    const save = registerServerAction(async (previous: Note, _form: FormData) => previous, ID);
    component Page() {
      const [note, submit] = useActionState<Note, FormData>(save, { saved: null });
      return (
        <form action={submit}>
          <output>{note.saved ?? "nothing saved"}</output>
        </form>
      );
    }
    const key = new Map(hiddenFields(await documentOf(Page))).get("$ACTION_KEY") ?? "";
    const html = await documentOf(Page, [{ saved: "not yours" }, key, OTHER, 0]);
    expect(html).toContain("<output>nothing saved</output>");
    expect(html).not.toContain("<output>not yours</output>");
  });
});

describe("the form state a document carries to hydrateRoot", () => {
  it("cannot close its own element, whatever the state holds", () => {
    const script = formStateScript([{ saved: "</script><script>alert(1)</script>" }, "k", ID, 0]);
    expect(script.indexOf("</script>")).toBe(script.length - "</script>".length);
  });

  it("is read back exactly, and a malformed one is read as none", () => {
    installDom();
    const state: FormState = [{ saved: "</script>" }, "k1", ID, 0];
    const parse = (html: string) => new globalThis.DOMParser().parseFromString(html, "text/html");
    expect(readFormState(parse(`<head>${formStateScript(state)}</head>`))).toEqual(state);
    expect(readFormState(parse("<head></head>"))).toBe(undefined);
    expect(
      readFormState(parse('<head><script type="application/json" id="uf:form-state">[1]</script>')),
    ).toBe(undefined);
  });
});

// ---------------------------------------------------------------------------
// The endpoint
// ---------------------------------------------------------------------------

/** A native post that passes every guard, with `fields` as its body. */
function posted(
  fields: $ReadOnlyArray<[string, string]>,
  headers?: { readonly [string]: string | null },
  at?: string = "https://app.example/notes?draft=1",
): Request {
  const given: { [string]: string } = {
    origin: "https://app.example",
    host: "app.example",
    "content-type": FORM_ACTION_CONTENT_TYPE,
    "sec-fetch-site": "same-origin",
  };
  for (const [name, value] of Object.entries(headers ?? {})) {
    if (value == null) delete given[name];
    else given[name] = String(value);
  }
  return new Request(at, {
    method: "POST",
    headers: given,
    body: new URLSearchParams(fields as $FlowFixMe).toString(),
  });
}

/** The fields React writes for one form, plus what the person typed. */
function formFor(
  typed: $ReadOnlyArray<[string, string]>,
  options?: {|
    readonly bound?: $ReadOnlyArray<mixed>,
    readonly key?: string,
    readonly id?: string,
  |},
): Array<[string, string]> {
  const fields: Array<[string, string]> = [
    ["$uf_ref_R0", ""],
    ["$uf_id_R0", options?.id ?? ID],
  ];
  if (options?.bound != null) {
    fields.push(["$uf_bound_R0", encodeActionArguments(options.bound)]);
  }
  if (options?.key != null) {
    fields.push(["$ACTION_KEY", options.key]);
  }
  return [...fields, ...typed];
}

/** Drive a dispatcher inside a request, as a host does. */
async function hosted(
  action: (...args: $FlowFixMe) => Promise<mixed>,
  request: Request,
  postback?: (formState: FormState) => Promise<Response>,
): Promise<Response | null> {
  const callAction = createActionDispatcher({
    actions: [
      {
        id: ID,
        module: "app/notes/_actions.js",
        export: "save",
        load: () => Promise.resolve({ save: action }),
      },
    ],
  });
  const { run, settle } = beginRequest(request);
  try {
    return await run(() => callAction(request, postback == null ? undefined : { postback }));
  } finally {
    await settle();
  }
}

describe("a form posted to a server action before its page hydrated", () => {
  it("calls the action with the form the person filled in, and nothing of the wiring", async () => {
    const calls = [];
    const answer = await hosted(
      async (form: FormData) => {
        calls.push([...form.entries()]);
      },
      posted(formFor([["note", "hello"]])),
    );
    expect(calls).toEqual([[["note", "hello"]]]);
    // Post/redirect/get, back to the page it came from, query and all.
    expect(answer?.status).toBe(303);
    expect(answer?.headers.get("location")).toBe("/notes?draft=1");
  });

  it("calls a useActionState action with its state, and answers with the page", async () => {
    const calls = [];
    let rendered: FormState | null = null;
    const answer = await hosted(
      async (previous: Note, form: FormData): Promise<Note> => {
        calls.push(previous);
        return { saved: String(form.get("note")) };
      },
      posted(formFor([["note", "hello"]], { bound: [{ saved: "before" }], key: "k0" })),
      async (formState) => {
        rendered = formState;
        return new Response("the page", { status: 200 });
      },
    );
    expect(calls).toEqual([{ saved: "before" }]);
    expect(rendered).toEqual([{ saved: "hello" }, "k0", ID, 0]);
    expect(answer?.status).toBe(200);
    expect(await answer?.text()).toBe("the page");
  });

  it("still runs the action when the host has no page to answer with", async () => {
    let ran = false;
    const answer = await hosted(
      async () => {
        ran = true;
        return { saved: "x" };
      },
      posted(formFor([], { bound: [null], key: "k0" })),
    );
    expect(ran).toBe(true);
    expect(answer?.status).toBe(303);
  });

  // A page reached at `https://app.example//evil.example/notes` posts its form
  // to the same address, and a request-target that opens with two slashes is
  // one every host but Node's passes through as it came. Answered with
  // `Location: //evil.example/notes`, the post/redirect/get would hand the
  // person to another host after they submitted a form on this one.
  it("answers back to a path on this origin, whatever the posted path opened with", async () => {
    for (const at of [
      "https://app.example//evil.example/notes?draft=1",
      "https://app.example/\\evil.example/notes?draft=1",
    ]) {
      const answer = await hosted(async () => {}, posted(formFor([]), undefined, at));
      expect(answer?.status).toBe(303);
      expect(answer?.headers.get("location")).toBe("/evil.example/notes?draft=1");
    }
  });

  it("follows redirect() with a 303, which a browser follows with a GET", async () => {
    const answer = await hosted(
      async () => {
        throw new RedirectError("/done", false);
      },
      posted(formFor([])),
    );
    expect(answer?.status).toBe(303);
    expect(answer?.headers.get("location")).toBe("/done");
  });

  it("lets the action turn draft mode on, and puts the cookie on the answer", async () => {
    const answer = await hosted(
      async () => {
        draftMode().enable();
      },
      posted(formFor([])),
    );
    expect(answer?.status).toBe(303);
    expect(answer?.headers.get("set-cookie") ?? "").toContain("uf.draft=");
  });

  it("says nothing about an action that threw", async () => {
    const answer = await hosted(
      async () => {
        throw new Error("the database password is hunter2");
      },
      posted(formFor([])),
    );
    expect(answer?.status).toBe(500);
    expect(await answer?.text()).not.toContain("hunter2");
  });

  it("refuses a post from another origin, from no origin, and from another site", async () => {
    const run = async () => undefined;
    const cases = [
      { origin: "https://evil.example" },
      { origin: null },
      { origin: "null" },
      { "sec-fetch-site": "cross-site" },
      { "sec-fetch-site": "same-site" },
    ];
    for (const headers of cases) {
      let ran = false;
      const answer = await hosted(
        async () => {
          ran = true;
          await run();
        },
        posted(formFor([]), headers),
      );
      expect([JSON.stringify(headers), answer?.status, ran]).toEqual([
        JSON.stringify(headers),
        403,
        false,
      ]);
    }
  });

  it("accepts a browser that sends no fetch metadata, on Origin alone", async () => {
    const answer = await hosted(
      async () => undefined,
      posted(formFor([]), { "sec-fetch-site": null }),
    );
    expect(answer?.status).toBe(303);
  });

  it("answers an id nobody has exactly as the JSON door does", async () => {
    for (const id of [OTHER, "not-an-id", ""]) {
      const answer = await hosted(async () => undefined, posted(formFor([], { id })));
      expect(answer?.status).toBe(404);
      expect(await answer?.text()).toBe('{"error":"server action refused"}');
    }
  });

  it("holds the bound arguments and the form to the wire's grammar", async () => {
    const bad = [
      // A bound envelope that is not the grammar.
      [
        ...formFor([]).filter(([n]) => n !== "$uf_bound_R0"),
        ["$uf_bound_R0", '{"args":[1],"x":2}'],
      ],
      [...formFor([]), ["$uf_bound_R0", "not json"]],
      [...formFor([]), ["$uf_bound_R0", '{"args":[{"__proto__":{}}]}']],
      // More fields than a form may carry.
      formFor(Array.from({ length: 300 }, (_, i) => [`f${String(i)}`, "v"])),
      // A field name longer than a form may carry.
      formFor([["n".repeat(200), "v"]]),
    ];
    for (const fields of bad) {
      let ran = false;
      const answer = await hosted(
        async () => {
          ran = true;
        },
        posted(fields as $FlowFixMe),
      );
      expect([answer?.status, ran]).toEqual([400, false]);
    }
  });

  it("leaves every post that is not an action post to somebody else, body unread", async () => {
    const plain = posted([["note", "hello"]]);
    expect(await hosted(async () => undefined, plain)).toBe(null);
    // The route handler that gets it next still has its body.
    expect(await plain.text()).toBe("note=hello");

    const multipart = new Request("https://app.example/notes", {
      method: "POST",
      headers: { origin: "https://app.example", host: "app.example" },
      body: (() => {
        const form = new FormData();
        form.append("$uf_ref_R0", "");
        form.append("$uf_id_R0", ID);
        return form;
      })(),
    });
    expect(await hosted(async () => undefined, multipart)).toBe(null);

    const get = new Request("https://app.example/notes?$uf_ref_R0=&$uf_id_R0=" + ID);
    expect(await hosted(async () => undefined, get)).toBe(null);
  });

  it("reads the last action named, which is the button that was pressed", () => {
    const post = readFormPost(
      new URLSearchParams([
        ["$uf_ref_R0", ""],
        ["$uf_id_R0", ID],
        ["note", "hi"],
        ["$uf_id_R1", OTHER],
        ["$uf_ref_R1", ""],
      ]),
    );
    expect(post?.id).toBe(OTHER);
    expect(post?.entries).toEqual([["note", "hi"]]);
  });
});
