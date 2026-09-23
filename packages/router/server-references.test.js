// @flow
//
// A server function a Server Component hands to a Client Component as a prop.
//
// Flight writes such a function as a *server reference*: an id and a promise of
// its bound arguments. Two readers turn that back into a function, and each has
// a section here, driven with a payload written by hand so that what is under
// test is uf's half and not React's renderer:
//
// 1. **The HTML renderer** reads the payload with React's edge client, and the
//    function it gets must write a form that posts before hydration — the same
//    fields an imported action writes (`./internal/form-action.js`).
// 2. **The browser** reads it with React's browser client, and calling the
//    function must be the call an imported action makes: the JSON wire, the
//    action id in `uf-action`, bound arguments first.
//
// The half that needs React's `react-server` build — registering the function
// so Flight writes a reference rather than refusing it — only loads under that
// export condition, so it is proved on a real build:
// `crates/uf_cli/tests/vite.rs` renders `tests/fixtures/rsc-split-app`, where a
// Server Component passes an action to a Client Component.

import * as React from "react";
import { prerender } from "react-dom/static";
import { describe, expect, it } from "@uniflowed/test";

import { installDom } from "../../packages/react-testing/internal/dom.js";
import { ACTION_HEADER, encodeActionArguments } from "./internal/action-wire.js";
import { callServerFunction } from "./internal/flight-browser.js";
import { encodeFormAction, readPayload } from "./internal/flight-ssr.js";
import { FORM_ACTION_CONTENT_TYPE } from "./internal/form-action.js";

const ID = "0f1e2d3c4b5a".repeat(6).slice(0, 64);

/**
 * A payload whose root is `{ action }`, a server reference bound to `bound`.
 *
 * Always with a bound row, empty when nothing is bound, because that is what
 * `registerServerFunction` makes Flight write.
 */
function payloadWith(bound: $ReadOnlyArray<mixed>): ReadableStream<Uint8Array> {
  const rows = [
    `2:${JSON.stringify(bound)}`,
    `1:${JSON.stringify({ id: ID, bound: "$@2" })}`,
    `0:{"action":"$h1"}`,
  ];
  const bytes = new TextEncoder().encode(`${rows.join("\n")}\n`);
  return new ReadableStream({
    start(controller) {
      controller.enqueue(bytes);
      controller.close();
    },
  });
}

/** The decoded `action`, as a function, or a failure that says what it was. */
async function actionIn(bound: $ReadOnlyArray<mixed>): Promise<$FlowFixMe> {
  const root: $FlowFixMe = await readPayload(payloadWith(bound));
  const action = root.action;
  if (typeof action !== "function") {
    throw new Error(`the payload decoded to ${String(action)}, not a function`);
  }
  return action;
}

/** `$$FORM_ACTION`, waited out the way React's HTML renderer waits on it. */
async function fieldsOf(action: $FlowFixMe): Promise<$FlowFixMe> {
  for (let attempt = 0; attempt < 5; attempt += 1) {
    try {
      return action.$$FORM_ACTION("ignored");
    } catch (thrown) {
      if (thrown == null || typeof thrown.then !== "function") throw thrown;
      await thrown;
    }
  }
  throw new Error("the bound arguments never settled");
}

/** Everything a prerender writes for `node`. */
async function markupOf(node: React.Node): Promise<string> {
  const { prelude } = await prerender(node);
  return await new Response(prelude).text();
}

describe("a server reference, while the HTML renders", () => {
  it("writes the fields an imported action writes, named by the action's id", async () => {
    const fields = await fieldsOf(await actionIn([]));
    expect(fields.method).toBe("POST");
    expect(fields.encType).toBe(FORM_ACTION_CONTENT_TYPE);
    const prefix = fields.name.slice("$uf_ref_".length);
    expect(fields.name.startsWith("$uf_ref_")).toBe(true);
    expect([...fields.data.entries()]).toEqual([[`$uf_id_${prefix}`, ID]]);
  });

  it("carries the arguments the Server Component bound", async () => {
    const fields = await fieldsOf(await actionIn(["note-7"]));
    const prefix = fields.name.slice("$uf_ref_".length);
    expect(fields.data.get(`$uf_bound_${prefix}`)).toBe(encodeActionArguments(["note-7"]));
  });

  it("keeps two bindings of one function apart, and one binding the same", async () => {
    const first = await fieldsOf(await actionIn(["a"]));
    const second = await fieldsOf(await actionIn(["b"]));
    const again = await fieldsOf(await actionIn(["a"]));
    expect(first.name).not.toBe(second.name);
    expect(first.name).toBe(again.name);
  });

  it("is a real form in the document, not a javascript: URL", async () => {
    const action = await actionIn(["note-7"]);
    const html = await markupOf(
      <form action={action}>
        <button type="submit">delete</button>
      </form>,
    );
    expect(html).not.toContain("javascript:");
    expect(html).toContain('method="POST"');
    expect(html).toContain(`value="${ID}"`);
  });

  it("refuses bound arguments the wire could not carry, rather than guessing", () => {
    // React catches this and writes the form it writes for a client action.
    const bound = Promise.resolve([new Date(0)]);
    // First call registers the promise; wait for it, then ask again.
    expect(() => encodeFormAction(ID, bound)).toThrow();
    return bound.then(() => {
      expect(() => encodeFormAction(ID, bound)).toThrow("class instance");
    });
  });
});

describe("a server reference, called in the browser", () => {
  /** Call through `callServerFunction` with `fetch` answered by `answer`. */
  async function called(
    args: Array<mixed>,
    answer: Response,
  ): Promise<{| +request: Request, +result: mixed |}> {
    installDom();
    globalThis.history.pushState(null, "", "/notes?page=2");
    const original = globalThis.fetch;
    let seen: Request | null = null;
    globalThis.fetch = async (input: $FlowFixMe, init: $FlowFixMe) => {
      seen = new Request(new URL(String(input), "http://localhost/"), init);
      return answer;
    };
    try {
      const result = await callServerFunction(ID, args);
      if (seen == null) throw new Error("nothing was sent");
      return { request: seen, result };
    } finally {
      globalThis.fetch = original;
    }
  }

  it("is the call an imported action makes: the page's URL, the id, the wire", async () => {
    const { request, result } = await called(
      ["note-7", "hello"],
      new Response('{"value":{"deleted":true}}', { status: 200 }),
    );
    expect(request.method).toBe("POST");
    expect(new URL(request.url).pathname + new URL(request.url).search).toBe("/notes?page=2");
    expect(request.headers.get(ACTION_HEADER)).toBe(ID);
    expect(request.headers.get("content-type")).toBe("application/json");
    expect(await request.text()).toBe(encodeActionArguments(["note-7", "hello"]));
    expect(result).toEqual({ deleted: true });
  });

  it("refuses at the call site what the wire cannot carry", async () => {
    installDom();
    await expect(callServerFunction(ID, [new Map()])).rejects.toThrow("class instance");
  });
});
