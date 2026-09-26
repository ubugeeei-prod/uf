// @flow
//
// The claims `docs/app/guide/server-actions/$page.mdx` makes that nothing else
// asserts (ubugeeei-prod/uf#1501).
//
// `tests/library/server-actions.test.js` covers the grammar and the endpoint
// row by row. These are the sentences its cases did not reach: the rarer
// values the grammar names as refused, the limit on how many values one
// payload holds, the body that is not UTF-8, the preflight nothing answers,
// the forwarded host nobody believes, and what a failure tells the operator
// and the browser. Each case would fail if its sentence stopped being true.

import * as React from "@uniflowed/react";

import { afterEach, describe, expect, it, uft } from "@uniflowed/test";

import { ServerActionError, createServerReference } from "./action.js";
import { createActionDispatcher } from "./internal/action-endpoint.js";
import {
  ACTION_CONTENT_TYPE,
  ACTION_HEADER,
  ActionValueError,
  MAX_ACTION_VALUES,
  encodeActionArguments,
} from "./internal/action-wire.js";
import { beginRequest } from "@uniflowed/server/host";

const ID = "c0ffee00".repeat(8);
const MODULE = "app/_actions/tally.js";

/** A dispatcher with one action, and a record of whether it ran. */
function endpoint(): {|
  readonly dispatch: (request: Request) => Promise<Response | null>,
  readonly ran: () => number,
|} {
  let calls = 0;
  const dispatch = createActionDispatcher({
    actions: [
      {
        id: ID,
        module: MODULE,
        export: "recordCount",
        load: async () => ({
          recordCount: async () => {
            calls += 1;
            return null;
          },
        }),
      },
    ],
  });
  return {
    dispatch: async (request) => {
      const { run, settle } = beginRequest(request);
      try {
        return await run(() => dispatch(request));
      } finally {
        await settle();
      }
    },
    ran: () => calls,
  };
}

/** A call that passes every guard, with `headers` applied last. */
function call(
  body: string | Uint8Array,
  headers?: { readonly [string]: string },
  method?: string,
): Request {
  const given: { [string]: string } = {
    origin: "https://app.example",
    host: "app.example",
    "content-type": ACTION_CONTENT_TYPE,
  };
  given[ACTION_HEADER] = ID;
  const extra: { readonly [string]: string } = headers ?? {};
  for (const name of Object.keys(extra)) given[name] = extra[name];
  const verb = method ?? "POST";
  return new Request("https://app.example/counter", {
    method: verb,
    headers: given,
    body: verb === "POST" ? body : undefined,
  });
}

describe("what may cross, beyond the common cases", () => {
  it("refuses a bigint, naming it", () => {
    expect(() => encodeActionArguments([BigInt(1)])).toThrow("is a bigint");
  });

  it("refuses a React element, which carries a symbol a payload cannot spell", () => {
    const element = <p />;
    expect(() => encodeActionArguments([element])).toThrow(ActionValueError);
  });

  it("refuses a value that holds the same object twice, not only one that holds itself", () => {
    const shared = { count: 1 };
    expect(() => encodeActionArguments([{ first: shared, second: shared }])).toThrow(
      "already appeared",
    );
  });

  it("refuses a symbol key, which JSON would drop without a word", () => {
    const value = { visible: 2 };
    Object.defineProperty(value, Symbol("hidden"), { value: 1, enumerable: true });
    expect(() => encodeActionArguments([value])).toThrow("symbol key");
  });

  it(`holds a payload to ${String(MAX_ACTION_VALUES)} values, on both sides of the wire`, async () => {
    const many = new Array<null>(MAX_ACTION_VALUES).fill(null);
    expect(() => encodeActionArguments([many])).toThrow("more than");

    const server = endpoint();
    const response = await server.dispatch(call(JSON.stringify({ args: [many] })));
    expect(response?.status).toBe(400);
    expect(server.ran()).toBe(0);
  });
});

describe("the body", () => {
  it("is refused, and the action not run, when it is not UTF-8", async () => {
    const server = endpoint();
    // `{"args":["` then a lone continuation byte, which no UTF-8 text contains.
    const bytes = new Uint8Array([
      ...new TextEncoder().encode('{"args":["'),
      0x80,
      0x22,
      0x5d,
      0x7d,
    ]);
    const response = await server.dispatch(call(bytes));
    expect(response?.status).toBe(400);
    expect(await response?.text()).toBe('{"error":"server action refused"}');
    expect(server.ran()).toBe(0);
  });
});

describe("the three cross-site guards", () => {
  it("answers no preflight: an OPTIONS naming an action is a 405, with no CORS grant", async () => {
    const server = endpoint();
    const response = await server.dispatch(
      call("", { "access-control-request-method": "POST" }, "OPTIONS"),
    );
    expect(response?.status).toBe(405);
    expect(response?.headers.get("allow")).toBe("POST");
    expect(response?.headers.get("access-control-allow-origin")).toBe(null);
    expect(response?.headers.get("access-control-allow-headers")).toBe(null);
    expect(server.ran()).toBe(0);
  });

  it("refuses an Origin that is not a URL", async () => {
    const server = endpoint();
    const response = await server.dispatch(call('{"args":[]}', { origin: "not a url" }));
    expect(response?.status).toBe(403);
    expect(server.ran()).toBe(0);
  });

  it("compares Origin with Host and never with X-Forwarded-Host", async () => {
    const server = endpoint();
    const forged = await server.dispatch(
      call('{"args":[]}', {
        origin: "https://evil.example",
        "x-forwarded-host": "evil.example",
      }),
    );
    expect(forged?.status).toBe(403);

    const proxied = await server.dispatch(
      call('{"args":[]}', { "x-forwarded-host": "somewhere.else.example" }),
    );
    expect(proxied?.status).toBe(200);
    expect(server.ran()).toBe(1);
  });
});

describe("what a failure says, and to whom", () => {
  afterEach(() => uft.restoreAllMocks());

  it("reports the exception with the module and export named, and answers with neither", async () => {
    const logged = uft.spyOn(console, "error").mockImplementation(() => {});
    const dispatch = createActionDispatcher({
      actions: [
        {
          id: ID,
          module: MODULE,
          export: "recordCount",
          load: async () => ({
            recordCount: async () => {
              throw new Error("the ledger is locked");
            },
          }),
        },
      ],
    });
    const request = call('{"args":[]}');
    const { run, settle } = beginRequest(request);
    const response = await run(() => dispatch(request));
    await settle();

    expect(response?.status).toBe(500);
    const body = (await response?.text()) ?? "";
    expect(body).not.toContain("recordCount");
    expect(body).not.toContain("ledger");
    const report = logged.mock.calls.map((args) => args.map(String).join(" ")).join("\n");
    expect(report).toContain("recordCount");
    expect(report).toContain(MODULE);
  });

  it("gives the browser a ServerActionError carrying the status and module#export", async () => {
    const fetched = uft
      .spyOn(globalThis, "fetch")
      .mockImplementation(
        async () => new Response('{"error":"server action refused"}', { status: 500 }),
      );
    // The reference posts to `location`, which a Node worker has none of; the
    // global is replaced for this case and put back after it.
    const scope: $FlowFixMe = globalThis;
    const hadLocation = Object.hasOwn(scope, "location");
    const previous = scope.location;
    scope.location = { pathname: "/counter", search: "", href: "https://app.example/counter" };
    try {
      const reference = createServerReference(ID, `${MODULE}#recordCount`);
      const failure = await reference().catch((error: mixed) => error);
      expect(failure).toBeInstanceOf(ServerActionError);
      if (!(failure instanceof ServerActionError)) return;
      expect(failure.status).toBe(500);
      expect(failure.action).toBe(`${MODULE}#recordCount`);
      expect(fetched).toHaveBeenCalledTimes(1);
    } finally {
      if (hadLocation) scope.location = previous;
      else delete scope.location;
    }
  });
});
