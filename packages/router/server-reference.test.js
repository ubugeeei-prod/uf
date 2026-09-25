// @flow
//
// A server action a Server Component handed to a Client Component, in the
// browser.
//
// Flight sends such an action as a server reference: its id and the arguments
// `.bind(null, …)` bound on the server (ubugeeei-prod/uf#1359). React's browser
// client decodes it into a function that calls the one callback
// `hydrateFlight` installs, and that callback has to be uf's action call: the
// same JSON wire, grammar and headers as a reference a client module imports,
// with the bound arguments first. The rsc half, where the action is registered
// and Flight writes the reference, runs over a real build in
// `crates/uf_cli/tests/fixtures/rsc-test-app/tests/notes-app.test.js`.

import { createFromReadableStream } from "react-server-dom-parcel/client.browser";

import { afterEach, describe, expect, it, uft } from "@uniflowed/test";

import { installDom } from "../../packages/react-testing/internal/dom.js";
import { ActionValueError, callServerReference } from "./action.js";
import { ACTION_HEADER, encodeActionResult } from "./internal/action-wire.js";
import { installServerCallback } from "./internal/flight-browser.js";

const globals: $FlowFixMe = globalThis;
const ID = "a1b2c3d4e5f6".repeat(6).slice(0, 64);
const NAME = "app/notes/_actions.js#remove";

afterEach(() => {
  uft.restoreAllMocks();
  if (globals.window != null) {
    globals.window.history.replaceState(null, "", "/");
  }
});

/** A page at `/notes/7` whose every action call answers `result`, recording each request. */
function pageAnswering(result: mixed): Array<Request> {
  installDom();
  globals.window.history.replaceState(null, "", "/notes/7");
  const sent: Array<Request> = [];
  uft.spyOn(globals, "fetch").mockImplementation(async (url: string, init: RequestOptions) => {
    sent.push(new Request(new URL(url, "http://localhost"), init));
    return new Response(encodeActionResult(result));
  });
  return sent;
}

/** A payload whose root holds `remove` as the server reference Flight writes for it. */
function payloadWith(bound: $ReadOnlyArray<mixed> | null): ReadableStream<Uint8Array> {
  const reference =
    bound == null
      ? `1:${JSON.stringify({ id: `${ID}#${NAME}`, bound: null })}\n`
      : `1:${JSON.stringify({ id: `${ID}#${NAME}`, bound: "$@2" })}\n2:${JSON.stringify(bound)}\n`;
  const text = `${reference}0:{"remove":"$h1"}\n`;
  return new ReadableStream({
    start(controller) {
      controller.enqueue(new TextEncoder().encode(text));
      controller.close();
    },
  });
}

/** The `remove` a payload's root decodes to. */
async function decodedRemove(
  bound: $ReadOnlyArray<mixed> | null,
): Promise<(...args: Array<mixed>) => Promise<mixed>> {
  installServerCallback();
  const root: $FlowFixMe = await createFromReadableStream(payloadWith(bound));
  expect(typeof root.remove).toBe("function");
  return root.remove;
}

describe("a server reference a payload sends", () => {
  it("calls the action over the JSON wire, under the action's id, from the page", async () => {
    const sent = pageAnswering("removed");
    const remove = await decodedRemove(null);

    expect(await remove("7")).toBe("removed");

    expect(sent.length).toBe(1);
    expect(sent[0].method).toBe("POST");
    expect(new URL(sent[0].url).pathname).toBe("/notes/7");
    expect(sent[0].headers.get(ACTION_HEADER)).toBe(ID);
    expect(sent[0].headers.get("content-type")).toBe("application/json");
    expect(await sent[0].text()).toBe('{"args":["7"]}');
  });

  it("sends the arguments bound on the server first", async () => {
    const sent = pageAnswering(null);
    const remove = await decodedRemove(["7", { soft: true }]);

    await remove("because");

    expect(await sent[0].text()).toBe('{"args":["7",{"soft":true},"because"]}');
  });

  it("refuses an argument outside the grammar before anything is sent", async () => {
    const sent = pageAnswering(null);

    let thrown: mixed = null;
    try {
      await callServerReference(`${ID}#${NAME}`, [new Map()]);
    } catch (error) {
      thrown = error;
    }

    expect(thrown instanceof ActionValueError).toBe(true);
    expect(sent).toEqual([]);
  });
});
