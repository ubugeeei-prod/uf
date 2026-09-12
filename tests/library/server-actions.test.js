// @flow
//
// Server actions: the grammar, the endpoint, and the tables the bundler builds.
//
// `crates/uf_rsc` has scanned `"use server"`, keyed every action and written a
// manifest since long before anything could call one. What is under test here
// is the other half — the bytes on the wire, the endpoint that reads them, and
// the two tables `@uniflowed/vite` builds out of that manifest so that the
// browser gets a reference where the server gets a function.
//
// Three seams, in the order a call crosses them:
//
// 1. `internal/action-wire.js` — what an argument and a result may be. Driven
//    directly, because it is the argument boundary and `docs/security.md` has
//    a row that points here. The form is part of it and has its own section:
//    it is the one prototype that crosses, and the tests are about the shape
//    that keeps it from being a tag in the value tree.
// 2. `createActionDispatcher` — every refusal, and the one path that is not
//    one. Driven with no server, no port and no build, the way
//    `route-handler.test.js` drives `createDispatcher`. A form call is asked
//    the same questions as every other, because it is the same request.
// 3. `packages/vite/internal/rsc.js` — the client's references and the
//    server's table, from a manifest, so that what the two halves agree on is
//    a file rather than a habit.
//
// The end-to-end proof is not here and could not be: it needs a build.
// `crates/uf_cli/tests/vite.rs` builds `tests/fixtures/rsc-split-app` and
// reads what came out, and its `every_adapter_answers_exactly_what_the_node_
// adapter_answers` calls a real action through all four deploy artefacts.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { describe, expect, it } from "@uniflowed/test";

import uniflowed from "../../packages/vite/index.js";
import { VIRTUAL } from "../../packages/vite/internal/routes.js";
import {
  ACTION_HEADER as VITE_ACTION_HEADER,
  RSC_MANIFEST_ENV,
  actionReferenceSource,
  actionsModuleSource,
  serverActionModules,
  serverActionTable,
} from "../../packages/vite/internal/rsc.js";
import {
  ACTION_CONTENT_TYPE,
  ACTION_HEADER,
  type ActionArgument,
  ActionValueError,
  MAX_ACTION_ARGUMENTS,
  MAX_ACTION_DEPTH,
  MAX_FORM_ENTRIES,
  MAX_FORM_NAME_LENGTH,
  checkActionValue,
  decodeActionArguments,
  decodeActionResult,
  encodeActionArguments,
  encodeActionResult,
  isActionId,
} from "../../packages/router/internal/action-wire.js";
import { beginRequest, createActionDispatcher } from "@uniflowed/router/server";
import { createFetchHandler } from "@uniflowed/server/fetch";
import { cookies, draftMode, headers } from "@uniflowed/server";

import { everyMisuseIsReported, repositoryRoot as repository } from "./type-tests.js";

/** An id shaped the way `uf_rsc::ActionId::to_hex` writes one. */
const idOf = (seed: string): string => seed.repeat(64).slice(0, 64);

const RECORD = idOf("a1b2c3d4e5f6");
const OTHER = idOf("9f8e7d6c5b4a");

/**
 * Drive a dispatcher inside a request, the way a host does.
 *
 * `createActionDispatcher` refuses outside one, for the same reason
 * `createDispatcher` does: an action that reads `cookies()` has to be
 * answering about the request it is inside.
 */
async function hosted(
  callAction: (request: Request) => Promise<Response | null>,
  request: Request,
): Promise<Response | null> {
  const { run, settle } = beginRequest(request);
  try {
    return await run(() => callAction(request));
  } finally {
    await settle();
  }
}

/** A request that passes every guard, with `overrides` applied last. */
function call(
  body: string,
  overrides?: { readonly [string]: string | null },
  method?: string,
): Request {
  const given: { [string]: string } = {
    origin: "https://app.example",
    host: "app.example",
    "content-type": ACTION_CONTENT_TYPE,
  };
  given[ACTION_HEADER] = RECORD;
  for (const [name, value] of Object.entries(overrides ?? {})) {
    if (value == null) {
      delete given[name];
    } else {
      given[name] = String(value);
    }
  }
  const verb = method ?? "POST";
  return new Request("https://app.example/counter", {
    method: verb,
    headers: given,
    body: verb === "GET" || verb === "HEAD" ? undefined : body,
  });
}

/** The table one action makes, over a module the test supplies. */
function tableFor(action: (...args: $FlowFixMe) => Promise<mixed>) {
  return [
    {
      id: RECORD,
      module: "app/_actions/tally.js",
      export: "recordCount",
      load: () => Promise.resolve({ recordCount: action }),
    },
    {
      id: OTHER,
      module: "app/_actions/tally.js",
      export: "clear",
      load: () => Promise.resolve({ clear: async () => undefined }),
    },
  ];
}

const status = async (response: Response | null): Promise<number> =>
  response == null ? -1 : response.status;

/** A `FormData` built from pairs, the way a submitted form arrives. */
function formOf(pairs: $ReadOnlyArray<[string, string]>): FormData {
  const form = new FormData();
  for (const [name, value] of pairs) {
    form.append(name, value);
  }
  return form;
}

/**
 * The decoded argument at `index`, as a `FormData`, or a failure that says so.
 *
 * A refinement rather than a cast: `decodeActionArguments` answers with
 * `ActionArgument`s, and the claim every form test is making is that this one
 * came back as a real `FormData` rather than as whatever was on the wire.
 */
function formAt(args: $ReadOnlyArray<ActionArgument>, index: number): FormData {
  const value = args[index];
  if (!(value instanceof FormData)) {
    throw new Error(`argument ${String(index + 1)} decoded as ${String(value)}, not a FormData`);
  }
  return value;
}

// ---------------------------------------------------------------------------
// The grammar
// ---------------------------------------------------------------------------

describe("what may cross to a server action", () => {
  it("carries the JSON data types and refuses everything else", () => {
    // Everything in the grammar, in one value, so a change that narrows it
    // fails here rather than in whichever application first noticed.
    checkActionValue(
      { name: "ada", age: 36, admin: true, tags: ["a", "b"], manager: null, nested: { deep: [1] } },
      "argument 1",
    );
    checkActionValue([], "argument 1");
    checkActionValue(null, "argument 1");

    for (const outside of [
      () => {},
      undefined,
      Symbol("s"),
      new Map<string, string>(),
      new Set<string>(),
      new Date(),
      /pattern/,
      new Uint8Array(2),
      Number.NaN,
      Number.POSITIVE_INFINITY,
    ]) {
      expect(() => checkActionValue(outside, "argument 1")).toThrow(ActionValueError);
    }
  });

  it("names where the offending value was, so the caller can find it", () => {
    let said = "";
    try {
      checkActionValue({ user: { createdAt: new Date() } }, "argument 2");
    } catch (error) {
      said = error instanceof Error ? error.message : "";
    }
    expect(said).toContain("argument 2.user.createdAt");
    expect(said).toContain("class instance");
  });

  it("refuses a key that becomes prototype pollution one line later", () => {
    // `JSON.parse` gives `__proto__` an own property rather than a prototype,
    // so this is not pollution yet — it is pollution in the first spread or
    // merge downstream, which is why the refusal is here and not there.
    expect(() => decodeActionArguments('{"args":[{"__proto__":{"admin":true}}]}')).toThrow(
      ActionValueError,
    );
    expect(() => decodeActionArguments('{"args":[{"constructor":1}]}')).toThrow(ActionValueError);
    expect(() => decodeActionArguments('{"args":[{"prototype":1}]}')).toThrow(ActionValueError);
  });

  it("refuses a value that refers to itself rather than encoding a reference", () => {
    const cyclic: { self?: mixed } = {};
    cyclic.self = cyclic;
    expect(() => checkActionValue(cyclic, "argument 1")).toThrow(ActionValueError);
  });

  it("bounds the nesting, iteratively, so a deep payload is refused and not fatal", () => {
    // Built to just over the ceiling. A recursive walk would be a stack
    // overflow here with the sender's hand on the depth.
    let deep: mixed = 1;
    for (let index = 0; index < MAX_ACTION_DEPTH + 2; index += 1) {
      deep = [deep];
    }
    expect(() => checkActionValue(deep, "argument 1")).toThrow(ActionValueError);

    let allowed: mixed = 1;
    for (let index = 0; index < MAX_ACTION_DEPTH - 1; index += 1) {
      allowed = [allowed];
    }
    checkActionValue(allowed, "argument 1");
  });

  it("bounds how many arguments a call may pass", () => {
    const many = Array.from({ length: MAX_ACTION_ARGUMENTS + 1 }, () => 1);
    expect(() => encodeActionArguments(many)).toThrow(ActionValueError);
    expect(() => decodeActionArguments(JSON.stringify({ args: many }))).toThrow(ActionValueError);
  });

  it("accepts exactly one body shape and refuses every other", () => {
    expect(decodeActionArguments('{"args":[1,"two"]}')).toEqual([1, "two"]);
    for (const wrong of [
      "not json",
      "[1,2]",
      "null",
      '"a string"',
      '{"args":1}',
      '{"args":[1],"extra":2}',
      "{}",
    ]) {
      expect(() => decodeActionArguments(wrong)).toThrow(ActionValueError);
    }
  });

  it("keeps `undefined` distinct from `null` on the way back", () => {
    // An action declared `Promise<void>` must not resolve to `null` in the
    // browser, and JSON has no spelling for the difference.
    expect(encodeActionResult(undefined)).toBe("{}");
    expect(decodeActionResult("{}")).toBe(undefined);
    expect(encodeActionResult(null)).toBe('{"value":null}');
    expect(decodeActionResult('{"value":null}')).toBe(null);
    expect(decodeActionResult('{"value":{"total":9}}')).toEqual({ total: 9 });
  });

  it("checks the answer too, because a browser is talking to whatever answered", () => {
    expect(() => decodeActionResult('{"value":1,"other":2}')).toThrow(ActionValueError);
    expect(() => decodeActionResult("[1]")).toThrow(ActionValueError);
  });

  it("recognises only the canonical spelling of an id", () => {
    expect(isActionId(RECORD)).toBe(true);
    expect(isActionId(RECORD.toUpperCase())).toBe(false);
    expect(isActionId(RECORD.slice(0, 63))).toBe(false);
    expect(isActionId(`${RECORD}0`)).toBe(false);
    expect(isActionId(`${RECORD.slice(0, 63)}g`)).toBe(false);
    // Two headers of the same name arrive joined with a comma, which is one
    // of the ways a caller can make this string not be an id.
    expect(isActionId(`${RECORD}, ${RECORD}`)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// The form
// ---------------------------------------------------------------------------

describe("a submitted form crossing to a server action", () => {
  it("writes the form beside the values, never inside one", () => {
    // The shape `useActionState` produces: the previous state, then the form.
    // `args[1]` is `null` and the form is the envelope's own key, so nothing
    // in the value tree is a tag saying which constructor to call.
    const body = encodeActionArguments([{ saved: "old" }, formOf([["note", "hi"]])]);
    expect(JSON.parse(body)).toEqual({
      args: [{ saved: "old" }, null],
      form: { at: 1, entries: [["note", "hi"]] },
    });
  });

  it("arrives on the other side as a real FormData at the argument it was", () => {
    const sent = formOf([
      ["note", "hello"],
      ["tag", "a"],
      ["tag", "b"],
    ]);
    const decoded = decodeActionArguments(encodeActionArguments([null, sent]));
    expect(decoded.length).toBe(2);
    expect(decoded[0]).toBe(null);
    const form = formAt(decoded, 1);
    expect(form.get("note")).toBe("hello");
    // A repeated name is two entries and not one, which is what a checkbox
    // group and a multiple `<select>` produce.
    expect(form.getAll("tag")).toEqual(["a", "b"]);
  });

  it("carries an empty form, which is what an empty submit is", () => {
    const decoded = decodeActionArguments(encodeActionArguments([formOf([])]));
    expect(formAt(decoded, 0).get("note")).toBe(null);
  });

  it("refuses a second form, because the envelope says which one only once", () => {
    expect(() => encodeActionArguments([formOf([]), formOf([])])).toThrow(ActionValueError);
  });

  it("refuses a form nested inside a value, and says a form is an argument", () => {
    let said = "";
    try {
      encodeActionArguments([{ fields: formOf([["note", "hi"]]) }]);
    } catch (error) {
      said = error instanceof Error ? error.message : "";
    }
    expect(said).toContain("argument 1.fields");
    expect(said).toContain("only be an argument of a call");
  });

  it("refuses a file, naming the field that carried it", () => {
    const form = new FormData();
    form.append("note", "hi");
    form.append("avatar", new Blob(["bytes"]), "avatar.png");
    let said = "";
    try {
      encodeActionArguments([form]);
    } catch (error) {
      said = error instanceof Error ? error.message : "";
    }
    expect(said).toContain("avatar");
    expect(said).toContain("file");
  });

  it("bounds how many fields and how long a field name may be", () => {
    const many: Array<[string, string]> = [];
    for (let index = 0; index <= MAX_FORM_ENTRIES; index += 1) {
      many.push([`f${String(index)}`, "x"]);
    }
    expect(() => encodeActionArguments([formOf(many)])).toThrow(ActionValueError);
    expect(() =>
      encodeActionArguments([formOf([["n".repeat(MAX_FORM_NAME_LENGTH + 1), "x"]])]),
    ).toThrow(ActionValueError);
  });

  it("never answers with a form, because a form is not something a server sends", () => {
    expect(() => encodeActionResult(formOf([["note", "hi"]]))).toThrow(ActionValueError);
  });

  it("refuses every envelope that is not exactly this one", () => {
    for (const wrong of [
      // A `form` that is not an object, or is one with other keys.
      '{"args":[null],"form":null}',
      '{"args":[null],"form":[["note","hi"]]}',
      '{"args":[null],"form":{"at":0}}',
      '{"args":[null],"form":{"at":0,"entries":[],"extra":1}}',
      // An `at` that names no argument, or is not an index.
      '{"args":[null],"form":{"at":1,"entries":[]}}',
      '{"args":[null],"form":{"at":-1,"entries":[]}}',
      '{"args":[null],"form":{"at":0.5,"entries":[]}}',
      '{"args":[null],"form":{"at":"0","entries":[]}}',
      // An `at` naming a slot the payload also gives a value: two statements
      // about one argument, and no reason to pick either.
      '{"args":[{"note":"hi"}],"form":{"at":0,"entries":[]}}',
      // Entries that are not pairs of strings.
      '{"args":[null],"form":{"at":0,"entries":{"note":"hi"}}}',
      '{"args":[null],"form":{"at":0,"entries":[["note"]]}}',
      '{"args":[null],"form":{"at":0,"entries":[["note","hi","extra"]]}}',
      '{"args":[null],"form":{"at":0,"entries":[["note",1]]}}',
      '{"args":[null],"form":{"at":0,"entries":[[1,"hi"]]}}',
      '{"args":[null],"form":{"at":0,"entries":[null]}}',
      // And a third key, which is the thing keeping this closed as it grows.
      '{"args":[null],"form":{"at":0,"entries":[]},"extra":1}',
    ]) {
      expect(() => decodeActionArguments(wrong)).toThrow(ActionValueError);
    }
  });

  it("bounds the entries a payload declares, not only the ones a form built", () => {
    const entries = [];
    for (let index = 0; index <= MAX_FORM_ENTRIES; index += 1) {
      entries.push([`f${String(index)}`, "x"]);
    }
    expect(() =>
      decodeActionArguments(JSON.stringify({ args: [null], form: { at: 0, entries } })),
    ).toThrow(ActionValueError);
    expect(() =>
      decodeActionArguments(
        JSON.stringify({
          args: [null],
          form: { at: 0, entries: [["n".repeat(MAX_FORM_NAME_LENGTH + 1), "x"]] },
        }),
      ),
    ).toThrow(ActionValueError);
  });

  it("still refuses a prototype key, wherever the sender put it", () => {
    // A form's field names reach `append`, which stores them in a multimap and
    // not on an object — but the values beside it are ordinary arguments and
    // the grammar has not moved for them.
    expect(() =>
      decodeActionArguments(
        '{"args":[{"__proto__":{"admin":true}},null],"form":{"at":1,"entries":[]}}',
      ),
    ).toThrow(ActionValueError);
    // And a field literally named `__proto__` is a name in a multimap, which
    // is the one place it is harmless — so it crosses rather than being
    // refused, and this says so on purpose.
    const decoded = decodeActionArguments(
      '{"args":[null],"form":{"at":0,"entries":[["__proto__","x"]]}}',
    );
    const form = formAt(decoded, 0);
    expect(form.get("__proto__")).toBe("x");
  });
});

// ---------------------------------------------------------------------------
// The endpoint
// ---------------------------------------------------------------------------

describe("the endpoint a server action is dialled at", () => {
  it("calls the action the id names and answers with what it returned", async () => {
    let seen: mixed = null;
    const callAction = createActionDispatcher({
      actions: tableFor(async (count: number) => {
        seen = count;
        return { total: count * 2 };
      }),
    });

    const response = await hosted(callAction, call(JSON.stringify({ args: [4] })));
    expect(await status(response)).toBe(200);
    expect(seen).toBe(4);
    expect(response == null ? "" : await response.text()).toBe('{"value":{"total":8}}');
  });

  it("lets an action turn draft mode on, which is the other place that may", async () => {
    // A server action and a route handler are the two things that own a
    // response, so they are the two places `draftMode().enable()` is allowed.
    // A CMS whose "preview" is a button rather than a link ends up here rather
    // than in `route-handler.test.js`. ubugeeei-prod/uf#282.
    const callAction = createActionDispatcher({
      actions: tableFor(async () => {
        draftMode().enable();
        return "previewing";
      }),
    });

    const response = await hosted(callAction, call(JSON.stringify({ args: [] })));

    expect(response?.status).toBe(200);
    const set = (response?.headers.getSetCookie() ?? []).find((value) =>
      value.startsWith("__Host-uf.draft="),
    );
    expect(set == null).toBe(false);
    expect((set ?? "").includes("HttpOnly")).toBe(true);
  });

  it("does not put the cookie on a refusal, which ran no action at all", async () => {
    // The scope covers the action and not the guards above it: a `403` for a
    // cross-origin call must not carry a cookie, and nothing in that path could
    // have asked for one.
    const callAction = createActionDispatcher({
      actions: tableFor(async () => {
        draftMode().enable();
        return "previewing";
      }),
    });

    const response = await hosted(
      callAction,
      call(JSON.stringify({ args: [] }), { origin: "https://evil.example" }),
    );

    expect(response?.status).toBe(403);
    expect(response?.headers.getSetCookie() ?? []).toEqual([]);
  });

  it("runs the action inside the host's request, so it can read one", async () => {
    const callAction = createActionDispatcher({
      actions: tableFor(async () => ({
        visitor: cookies().get("visitor"),
        agent: headers().get("user-agent"),
      })),
    });

    const response = await hosted(
      callAction,
      call(JSON.stringify({ args: [] }), { cookie: "visitor=ada", "user-agent": "probe" }),
    );
    expect(response == null ? "" : await response.text()).toBe(
      '{"value":{"visitor":"ada","agent":"probe"}}',
    );
  });

  it("calls a form action with the state and the form React gave it", async () => {
    // The `useActionState` convention, end to end through the endpoint: the
    // previous state is an ordinary value, the submitted fields are the form,
    // and what comes back is the next state.
    let seen: mixed = null;
    const callAction = createActionDispatcher({
      actions: tableFor(async (previous: mixed, form: mixed) => {
        if (!(form instanceof FormData)) {
          throw new Error("the endpoint did not hand the action a FormData");
        }
        seen = form.get("note");
        const saved = typeof previous === "string" ? `${previous}+` : "";
        return { saved: `${saved}${String(form.get("note"))}` };
      }),
    });

    const body = encodeActionArguments(["old", formOf([["note", "hello"]])]);
    const response = await hosted(callAction, call(body));
    expect(await status(response)).toBe(200);
    expect(seen).toBe("hello");
    expect(response == null ? "" : await response.text()).toBe('{"value":{"saved":"old+hello"}}');
  });

  it("guards a form call with exactly the guards it applies to every other", async () => {
    // The whole reason a form crosses as `application/json` with the id in a
    // header: rule 4 of `action-endpoint.js` is about the *request*, and a
    // form call is the same request as any other. A cross-origin page cannot
    // reach this action by putting a `<form>` on itself.
    const callAction = createActionDispatcher({ actions: tableFor(async () => null) });
    const body = encodeActionArguments([formOf([["note", "hello"]])]);
    expect(
      await status(await hosted(callAction, call(body, { origin: "https://evil.example" }))),
    ).toBe(403);
    expect(
      await status(
        await hosted(callAction, call(body, { "content-type": "multipart/form-data; boundary=x" })),
      ),
    ).toBe(415);
    expect(await status(await hosted(callAction, call(body, undefined, "GET")))).toBe(405);
  });

  it("answers a malformed form envelope the way it answers any malformed payload", async () => {
    const callAction = createActionDispatcher({ actions: tableFor(async () => null) });
    const response = await hosted(callAction, call('{"args":[null],"form":{"at":9,"entries":[]}}'));
    expect(await status(response)).toBe(400);
    // The reason is the sender's own payload described back to them, which is
    // a thing for a log and not a thing to answer with.
    expect(response == null ? "" : await response.text()).toBe('{"error":"server action refused"}');
  });

  it("declines a request that names no action, so everything else is somebody else's", async () => {
    const callAction = createActionDispatcher({ actions: tableFor(async () => null) });
    expect(
      await hosted(callAction, call(JSON.stringify({ args: [] }), { [ACTION_HEADER]: null })),
    ).toBe(null);
  });

  it("refuses a call from another origin", async () => {
    const callAction = createActionDispatcher({ actions: tableFor(async () => null) });
    const body = JSON.stringify({ args: [] });
    expect(
      await status(await hosted(callAction, call(body, { origin: "https://evil.example" }))),
    ).toBe(403);
    // Deny by default: absent is not the same fact as matching.
    expect(await status(await hosted(callAction, call(body, { origin: null })))).toBe(403);
    expect(await status(await hosted(callAction, call(body, { origin: "null" })))).toBe(403);
    // The port is part of an origin, and a cookie tells two of them apart.
    expect(
      await status(await hosted(callAction, call(body, { origin: "https://app.example:8443" }))),
    ).toBe(403);
  });

  it("refuses a content type a cross-origin form could have produced", async () => {
    const callAction = createActionDispatcher({ actions: tableFor(async () => null) });
    for (const type of [
      "application/x-www-form-urlencoded",
      "multipart/form-data; boundary=x",
      "text/plain",
      "application/ld+json",
    ]) {
      expect(await status(await hosted(callAction, call("args=1", { "content-type": type })))).toBe(
        415,
      );
    }
    // The parameters after the media type are not part of the comparison.
    expect(
      await status(
        await hosted(
          callAction,
          call(JSON.stringify({ args: [] }), { "content-type": "application/JSON; charset=utf-8" }),
        ),
      ),
    ).toBe(200);
  });

  it("answers a method other than POST with 405 and the Allow it requires", async () => {
    const callAction = createActionDispatcher({ actions: tableFor(async () => null) });
    const response = await hosted(callAction, call("", undefined, "GET"));
    expect(await status(response)).toBe(405);
    expect(response == null ? "" : response.headers.get("allow")).toBe("POST");
  });

  it("answers every failed lookup identically, whatever was wrong with the id", async () => {
    const callAction = createActionDispatcher({ actions: tableFor(async () => null) });
    const body = JSON.stringify({ args: [] });
    const answers = [];
    for (const id of [idOf("00000000"), "too-short", `${RECORD}!`, ""]) {
      const response = await hosted(callAction, call(body, { [ACTION_HEADER]: id }));
      answers.push(
        `${String(await status(response))} ${response == null ? "" : await response.text()}`,
      );
    }
    // One answer, so the endpoint cannot be used to enumerate what a build
    // contains. `ServerActionRegistry::resolve` keeps the same contract on the
    // other side of the wire.
    expect(new Set(answers).size).toBe(1);
    expect(answers[0]).toBe('404 {"error":"server action refused"}');
  });

  it("answers a malformed payload before it has decided whether the id exists", async () => {
    const callAction = createActionDispatcher({ actions: tableFor(async () => null) });
    // Same body, one id that exists and one that does not: both 400, so a
    // caller cannot try a bad payload against a guess to learn whether the
    // guess was a real action.
    const known = await hosted(callAction, call("{"));
    const unknown = await hosted(callAction, call("{", { [ACTION_HEADER]: idOf("00000000") }));
    expect(await status(known)).toBe(400);
    expect(await status(unknown)).toBe(400);
  });

  it("refuses a body larger than an action accepts, without reading all of it", async () => {
    const callAction = createActionDispatcher({ actions: tableFor(async () => null) });
    // An honest `Content-Length` is refused before the body is transferred.
    const declared = call(JSON.stringify({ args: [] }), { "content-length": "9999999" });
    expect(await status(await hosted(callAction, declared))).toBe(413);
    // And so is a body that is simply larger than it said.
    const big = JSON.stringify({ args: ["x".repeat(1024 * 1024 + 64)] });
    expect(await status(await hosted(callAction, call(big)))).toBe(413);
  });

  it("says nothing about an action that threw", async () => {
    const callAction = createActionDispatcher({
      actions: tableFor(async () => {
        throw new Error("the database password is hunter2");
      }),
    });
    const response = await hosted(callAction, call(JSON.stringify({ args: [] })));
    expect(await status(response)).toBe(500);
    const said = response == null ? "" : await response.text();
    expect(said).toBe('{"error":"server action refused"}');
    expect(said).not.toContain("hunter2");
  });

  it("refuses to answer with a result that cannot cross", async () => {
    // Flow says so at build time — `ServerActionResultsFitTheWire` in the
    // generated `server-actions.js` — so this is the case where that was
    // reached anyway, and half a value is worse than none.
    const callAction = createActionDispatcher({
      actions: tableFor(async () => new Map<string, string>([["a", "b"]])),
    });
    expect(await status(await hosted(callAction, call(JSON.stringify({ args: [] }))))).toBe(500);
  });

  it("never caches an answer", async () => {
    const callAction = createActionDispatcher({ actions: tableFor(async () => null) });
    const response = await hosted(callAction, call(JSON.stringify({ args: [] })));
    expect(response == null ? "" : response.headers.get("cache-control")).toBe("no-store");
  });

  it("refuses outside a request, naming what establishes one", async () => {
    const callAction = createActionDispatcher({ actions: tableFor(async () => null) });
    await expect(callAction(call(JSON.stringify({ args: [] })))).rejects.toThrow(
      "callAction() was called outside a request",
    );
  });

  it("has no endpoint at all when the build declared no action", async () => {
    const callAction = createActionDispatcher({ actions: [] });
    expect(await status(await hosted(callAction, call(JSON.stringify({ args: [] }))))).toBe(404);
  });
});

// ---------------------------------------------------------------------------
// The tables the bundler builds
// ---------------------------------------------------------------------------

describe("the two tables the manifest becomes", () => {
  const root = "/project";
  const manifest = {
    version: 3,
    serverActions: [
      {
        id: RECORD,
        module: "app/_actions/tally.js",
        export: "recordCount",
        kind: "module-export",
      },
      { id: OTHER, module: "app/_actions/tally.js", export: "default", kind: "module-export" },
      {
        id: idOf("cccccccc"),
        module: "app/_actions/tally.js",
        export: "onClick",
        kind: "inline-closure",
      },
      { id: "not-an-id", module: "app/_actions/tally.js", export: "forged", kind: "module-export" },
      {
        id: idOf("dddddddd"),
        module: "../outside/tally.js",
        export: "escape",
        kind: "module-export",
      },
    ],
  };

  it("gives the browser one reference per callable export and nothing else", () => {
    const modules = serverActionModules(manifest, root);
    const rows = modules.get("/project/app/_actions/tally.js");
    expect(rows == null ? 0 : rows.length).toBe(2);

    const source = actionReferenceSource(rows ?? []);
    expect(source).toContain('import { createServerReference } from "@uniflowed/router/action";');
    expect(source).toContain(
      `export const recordCount = createServerReference(${JSON.stringify(RECORD)}, ` +
        '"app/_actions/tally.js#recordCount");',
    );
    // `default` is an export like any other and needs the other spelling.
    expect(source).toContain(
      `export default createServerReference(${JSON.stringify(OTHER)}, ` +
        '"app/_actions/tally.js#default");',
    );
    // Nothing of the module itself: not its imports, not its body.
    expect(source).not.toContain("import { cookies }");
  });

  it("leaves out what has no name to import and what is not an id", () => {
    const rows = serverActionModules(manifest, root).get("/project/app/_actions/tally.js") ?? [];
    const names = rows.map((row) => row.export);
    // An inline `"use server"` closure is an action and is not an export, so
    // there is nothing to write a reference for; it stays in the manifest.
    expect(names).not.toContain("onClick");
    // And a row whose id is not the canonical spelling is not a row: a table
    // built from a file uf wrote is still a file, and a forged one must not
    // become an endpoint.
    expect(names).not.toContain("forged");
  });

  it("refuses a module path that leaves the project", () => {
    const table = serverActionTable(manifest, root);
    expect(table.map((row) => row.module)).not.toContain("../outside/tally.js");
    expect(serverActionModules(manifest, root).has("/outside/tally.js")).toBe(false);
  });

  it("gives the server one import per file, however many actions are in it", () => {
    const source = actionsModuleSource(serverActionTable(manifest, root));
    expect(source.split('() => import("/project/app/_actions/tally.js")').length - 1).toBe(1);
    expect(source).toContain(`id: ${JSON.stringify(RECORD)}`);
    expect(source).toContain('export: "recordCount"');
  });

  it("builds no table at all from a manifest it cannot read", () => {
    // The same rule the route split follows: uf will not guess at an analysis
    // it was not given, so a project driving Vite itself gets no endpoint
    // rather than one built out of nothing.
    for (const absent of [null, { version: 1, serverActions: [] }, {}]) {
      expect(serverActionTable(absent, root)).toEqual([]);
      expect(serverActionModules(absent, root).size).toBe(0);
    }
    expect(actionsModuleSource([])).toContain("export const actions = [\n\n];");
  });

  it("spells the request header the same way on both sides of the transform", () => {
    // `@uniflowed/vite` is plain JavaScript the Vite host imports before any
    // transform, and `internal/action-wire.js` is Flow, which Node cannot
    // import — so the name is written twice and this is what keeps it one
    // fact. `RSC_MANIFEST_ENV` is the same situation with Rust.
    expect(VITE_ACTION_HEADER).toBe(ACTION_HEADER);
  });
});

// ---------------------------------------------------------------------------
// What the checker says about an action
// ---------------------------------------------------------------------------

describe("an action misused, held to what the checker actually says", () => {
  // The two claims a server action makes about types, and neither is provable
  // by calling anything. The first: a wrong argument is caught at the call
  // site, in the browser as on the server, because Flow reads the module's own
  // declaration and never the reference the bundler substitutes for it. The
  // second: an action whose arguments or result cannot cross a wire is caught
  // at all — a `createUser(when: Date)` type-checks at every call and then
  // arrives on the server as `{}`.
  //
  // `tests/type-tests/server-actions.js` is both, written down. It is
  // *supposed* to fail `uf check`, it marks each line that must fail with a
  // `// expect:` comment, and `./type-tests.js` reads both and compares them.

  it("reports every misuse, and only the misuses", () => {
    everyMisuseIsReported({
      fixture: path.join("tests", "type-tests", "server-actions.js"),
      alongside: ["packages/router"],
      atLeast: 4,
    });
  });
});

// ---------------------------------------------------------------------------
// Where the endpoint sits in a host
// ---------------------------------------------------------------------------

describe("the order every host runs an action in", () => {
  /** A host, through the seam all four deploy adapters answer out of. */
  function host(app: $FlowFixMe) {
    const handle = createFetchHandler({
      app,
      document: { scripts: [], styles: [], preloads: [] },
    });
    return async (request: Request): Promise<Response> => {
      const { run, settle } = beginRequest(request);
      try {
        return await run(() => handle(request));
      } finally {
        await settle();
      }
    };
  }

  /** An application that records which step was reached, and answers nowhere. */
  function recording(steps: Array<string>, overrides?: $FlowFixMe) {
    return {
      beginRequest,
      runMiddleware: async () => {
        steps.push("middleware");
        return null;
      },
      callAction: async (request: Request) => {
        steps.push("action");
        return createActionDispatcher({ actions: tableFor(async () => "ran") })(request);
      },
      dispatch: async () => {
        steps.push("dispatch");
        return null;
      },
      render: async () => {
        steps.push("render");
        return { status: 404, stream: () => new Response("").body };
      },
      ...(overrides ?? {}),
    };
  }

  it("runs below the guard and above the route handlers", async () => {
    const steps: Array<string> = [];
    const response = await host(recording(steps))(call(JSON.stringify({ args: [] })));
    expect(response.status).toBe(200);
    // Reached the action and stopped there: a `POST` that named an action must
    // not go on to a route handler that happens to sit at the same path.
    expect(steps).toEqual(["middleware", "action"]);
  });

  it("is below the guard, so a guard that answers is the whole request", async () => {
    const steps: Array<string> = [];
    const app = recording(steps, {
      runMiddleware: async () => {
        steps.push("middleware");
        return new Response(null, { status: 302, headers: { location: "/sign-in" } });
      },
    });
    const response = await host(app)(call(JSON.stringify({ args: [] })));
    expect(response.status).toBe(302);
    expect(steps).toEqual(["middleware"]);
  });

  it("declines a request that names no action, so a handler still gets it", async () => {
    const steps: Array<string> = [];
    const app = recording(steps, {
      dispatch: async () => {
        steps.push("dispatch");
        return new Response("handled", { status: 201 });
      },
    });
    const response = await host(app)(call(JSON.stringify({ args: [] }), { [ACTION_HEADER]: null }));
    expect(response.status).toBe(201);
    expect(steps).toEqual(["middleware", "action", "dispatch"]);
  });
});

// ---------------------------------------------------------------------------
// The plugin hook the two environments share
// ---------------------------------------------------------------------------

describe("the module the browser is given in place of a `use server` file", () => {
  // `uf dev` and `uf build` must agree about this, and the reason they do is
  // that there is one `load` hook and Vite calls it in both — what differs is
  // only which environment is asking. So the hook is driven directly, once per
  // environment, which is the whole of the difference between the two commands
  // for this decision. `crates/uf_cli/tests/vite.rs` asserts the consequence on
  // a real production bundle; `uf dev` reaches the same code with `ssr: false`.
  const root = path.join(repository, "crates", "uf_cli", "tests", "fixtures", "rsc-split-app");
  const action = path.join(root, "app", "counter", "_actions", "tally.js");
  const manifest = {
    version: 3,
    modules: [],
    serverActions: [
      {
        id: RECORD,
        module: "app/counter/_actions/tally.js",
        export: "recordCount",
        kind: "module-export",
      },
    ],
  };

  /** The `uf:flow` plugin, told where the project is, the way Vite tells it. */
  function flowPlugin() {
    const plugins: $FlowFixMe = uniflowed({ root });
    const flow = plugins.find((plugin) => plugin.name === "uf:flow");
    flow.configResolved({ root, base: "/" });
    return flow;
  }

  /** What `load` answers for `file` in one environment. */
  function loaded(file: string, ssr: boolean): mixed {
    const flow = flowPlugin();
    const was = process.env[RSC_MANIFEST_ENV];
    const written = path.join(
      fs.mkdtempSync(path.join(os.tmpdir(), "uf-actions-")),
      "uf-rsc-manifest.json",
    );
    fs.writeFileSync(written, JSON.stringify(manifest));
    process.env[RSC_MANIFEST_ENV] = written;
    try {
      return flow.load.call({ environment: { name: ssr ? "ssr" : "client" } }, file, { ssr });
    } finally {
      if (was == null) delete process.env[RSC_MANIFEST_ENV];
      else process.env[RSC_MANIFEST_ENV] = was;
      fs.rmSync(path.dirname(written), { recursive: true, force: true });
    }
  }

  it("hands the client a reference and the server the file itself", () => {
    const forBrowser = loaded(action, false);
    expect(typeof forBrowser).toBe("string");
    expect(String(forBrowser)).toContain(
      'import { createServerReference } from "@uniflowed/router/action";',
    );
    expect(String(forBrowser)).toContain(RECORD);
    // The module's own body is not in what the browser is handed, and neither
    // is the import that would have put `node:async_hooks` in its graph.
    expect(String(forBrowser)).not.toContain("tally-marker");
    expect(String(forBrowser)).not.toContain("@uniflowed/server");

    // `null` is Vite's "read the file", which is what the server wants: the
    // server executes the action, so the server gets the function.
    expect(loaded(action, true)).toBe(null);
  });

  it("leaves every other module alone in both environments", () => {
    const page = path.join(root, "app", "counter", "$page.js");
    expect(loaded(page, false)).toBe(null);
    expect(loaded(page, true)).toBe(null);
  });

  it("gives the browser no table of the build's endpoints", () => {
    // `virtual:uf/actions` is only imported by `virtual:uf/server`, so a
    // browser asking for it is not something that happens — and if it did, the
    // list of every callable id in the build is not the answer.
    const flow = flowPlugin();
    const id = `\0${VIRTUAL.actions}`;
    const forBrowser = flow.load.call({ environment: { name: "client" } }, id, { ssr: false });
    expect(String(forBrowser)).toBe("export const actions = [];\nexport default actions;\n");
  });
});
