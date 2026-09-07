// @flow
//
// Internal to `@uniflowed/router`: the endpoint a server action is dialled at.
//
// `createActionDispatcher` is the fourth thing `virtual:uf/server` exports and
// the third thing a host calls, between the middleware and the route handlers.
// It takes a table of `{ id, module, export, load }` built from the RSC
// manifest, and answers a request that carries an action id — or declines,
// with `null`, so that everything else about the request is somebody else's.
//
// # This is a public endpoint, and it is treated as one
//
// A `"use server"` export is reachable by anything that can make an HTTP
// request from the moment the build contains it. Every rule below is one of
// `docs/security.md`'s applied to that fact, and the table there has the row.
//
// **1. Only the build's own actions are dialable.** The table is the
// manifest's `serverActions`, which `uf_rsc` writes only for an action some
// module that can hand it across a client boundary reaches
// (`ActionExposure::CallableEndpoint`). A `"use server"` function nothing
// exposes has no row and therefore no endpoint. There is no path from a
// request to a module specifier, a file name or an export name: the id selects
// a row, and the row was decided at build time.
//
// **2. The id is not guessable.** `uf_rsc::ActionId` is
// `HMAC-SHA256(build id, module ‖ export ‖ kind)`, so the whole repository plus
// a published sourcemap is not enough to derive the id of a function the
// interface does not offer, and a rebuild changes every id.
//
// **3. One answer for every failed lookup.** `404`, with no body, for a
// malformed id, a well-formed id nobody has, and an id that names something not
// callable. `select` compares against every row whatever happens, and compares
// with `sameId`, so neither the answer nor the time taken says which. This is
// `ServerActionRegistry::resolve`'s contract, kept on the other side of the
// wire.
//
// **4. Cross-site calls cannot happen by accident, three times over.** The
// call is a `POST` carrying `uf-action`, which is not a header a simple request
// may set, so a cross-origin caller needs a preflight and nothing here answers
// one. It must be `application/json`, which is not a content type a `<form>`
// can produce. And `Origin` must be present and must equal `Host`. Any one of
// the three would do; all three are here because the cost is three `if`s and
// the failure is somebody's account.
//
// `Origin` is compared against `Host` and against nothing else. `Host` is what
// the browser was talking to, which is exactly the question being asked, and
// `X-Forwarded-Host` is a string the caller wrote (`docs/security.md`, rule 2)
// — so a proxy in front of a uf application has to preserve `Host`, and one
// that rewrites it turns every action call into a `403` rather than into a
// hole. A refusal is the right way for that to be discovered.
//
// **5. The argument boundary is `./action-wire.js` and only that.** Bounded
// bytes, bounded depth, bounded count, valid UTF-8, plain JSON data, no
// prototype keys, no constructor named by the payload. See that module's
// header for what is excluded and why.
//
// A submitted form is inside that boundary and does not widen it: `<form
// action={fn}>` reaches here as the same `application/json` body, with the
// form's entries beside the values under a `form` key, bounded in count and in
// field-name length and holding strings only. Nothing about it is multipart
// and nothing about it is a content type a cross-origin `<form>` could
// produce, so rule 4 is exactly as true of a form call as of any other. The
// cost of keeping it that way is written down where it is paid — a form that
// submits before its page has hydrated throws in the page rather than posting
// anywhere, because React writes `action="javascript:throw …"` for a form
// whose action carries no `$$FORM_ACTION`, and giving it one would mean
// accepting a native form post here.
//
// **6. Nothing about a failure goes back.** An action that throws is a `500`
// with a fixed body; the exception goes to the host's error reporting. A
// message, a name or a stack in that response is an application's internals
// published to whoever asked for them, and the same is true in development —
// `uf dev` and `uf build` have to agree about what this endpoint answers, and
// the terminal is where a developer reads the exception anyway.
//
// # What a middleware does and does not do for an action
//
// The call is a `POST` to the page's own URL rather than to a path uf reserves,
// so the middleware that guards that path runs above it exactly as it does for
// the page — no new route to collide with a project's own, and no second
// spelling of "which guard applies here".
//
// That is a convenience and it is not a boundary, because the URL is the
// caller's to choose: a client that wants to skip the guard on `/dashboard`
// posts the same id to `/`. **A server action authorizes itself.** It is the
// unit of authorization, the way a route handler is, and a `"use server"`
// function that relies on a path guard having run is a function with a hole in
// it. Written here because this is the file somebody reads before deciding
// otherwise.

import {
  ACTION_CONTENT_TYPE,
  ACTION_HEADER,
  type ActionArgument,
  ActionValueError,
  MAX_ACTION_BODY_BYTES,
  decodeActionArguments,
  encodeActionResult,
  isActionId,
} from "./action-wire.js";
import { requireRequest } from "./request.js";

/** A module holding server actions, as the generated table loads it. */
export type ActionModule = { readonly [name: string]: mixed };

/**
 * One callable action, as `virtual:uf/actions` writes it.
 *
 * `module` and `export` are here for the error a broken build produces, never
 * for dispatch: dispatch is `id` against `id`. Nothing a request carries is
 * ever joined onto a path or used to name an export.
 */
export type ActionRecord = {|
  /** The keyed id, 64 lowercase hexadecimal characters. */
  readonly id: string,
  /** Declaring module, relative to the project root. */
  readonly module: string,
  /** The exported binding, or `default`. */
  readonly export: string,
  /** Import the declaring module. */
  readonly load: () => Promise<ActionModule>,
|};

/** Headers every answer carries, whatever it says. */
const ANSWER_HEADERS: { readonly [string]: string } = {
  "content-type": "application/json; charset=utf-8",
  // An action call is a `POST` and is not cached by anything by default. Said
  // anyway, because "not cached by default" is the sentence in front of every
  // cache-poisoning advisory in `docs/security.md`.
  "cache-control": "no-store",
};

/**
 * Match a request against the action table and run what it names.
 *
 * Returns `null` when the request carries no action id, which is the caller's
 * signal to carry on: everything that is not an action call is a page, a
 * handler or a 404, and this declines all of them. Every other outcome —
 * including every refusal — is a `Response`, because a request that named an
 * action and did not get to call one must not fall through to something else
 * that might answer it.
 */
export function createActionDispatcher(options: {|
  readonly actions: $ReadOnlyArray<ActionRecord>,
|}): (request: Request) => Promise<Response | null> {
  const table = options.actions;

  return async function callAction(request: Request): Promise<Response | null> {
    // The host's half of the contract, checked rather than assumed, exactly as
    // `dispatch` and `runMiddleware` check it: an action that calls `cookies()`
    // has to answer about the request it is inside.
    requireRequest("callAction");

    const id = request.headers.get(ACTION_HEADER);
    if (id == null) {
      return null;
    }

    // Cheapest and most protective first, and all of it before a byte of the
    // body is read.
    if (request.method.toUpperCase() !== "POST") {
      return new Response(null, { status: 405, headers: { ...ANSWER_HEADERS, allow: "POST" } });
    }
    if (!sameOrigin(request)) {
      return refusal(403);
    }
    if (!isJson(request.headers.get("content-type"))) {
      return refusal(415);
    }

    const body = await readBoundedText(request, MAX_ACTION_BODY_BYTES);
    if (body == null) {
      return refusal(413);
    }

    let args: Array<ActionArgument>;
    try {
      args = decodeActionArguments(body);
    } catch (error) {
      // The reason is the sender's own payload described back to them, which
      // is a thing to write in a log and not a thing to answer with.
      if (!(error instanceof ActionValueError)) {
        throw error;
      }
      return refusal(400);
    }

    // Decoded before the lookup, so that a malformed payload and an unknown id
    // cannot be told apart by trying one against the other.
    const record = isActionId(id) ? select(table, id) : null;
    if (record == null) {
      return refusal(404);
    }

    let action: mixed;
    try {
      const module = await record.load();
      action = module[record.export];
    } catch (error) {
      report(record, error);
      return refusal(500);
    }
    if (typeof action !== "function") {
      // The RSC graph rejects a `"use server"` export that is not an async
      // function at build time, so reaching this means the manifest and the
      // modules disagree — a stale `.uf/rsc/uf-rsc-manifest.json`, or a build
      // half-written. It is uf's bug, not the caller's, so it is reported and
      // answered as a `500`.
      report(record, new Error(`export \`${record.export}\` is not a function`));
      return refusal(500);
    }

    let result: mixed;
    try {
      // The build-time contract says this is `async (...ActionArgument) => …`
      // (`ServerActionBoundary` in `../action.js`), and Flow cannot read that
      // through a module loaded by a thunk. The arguments are the ones
      // `decodeActionArguments` produced, so what is unchecked here is the
      // shape of the function and not the shape of the payload.
      const call = action as $FlowFixMe;
      result = await call(...args);
    } catch (error) {
      report(record, error);
      return refusal(500);
    }

    let answer: string;
    try {
      answer = encodeActionResult(result);
    } catch (error) {
      // The action ran and its return value cannot cross. Flow says so at
      // build time — `ServerActionBoundary` in `../action.js` holds every
      // action's return type against the grammar — so this is the case where
      // it was reached anyway, and half a value is worse than none.
      report(record, error);
      return refusal(500);
    }
    return new Response(answer, { status: 200, headers: { ...ANSWER_HEADERS } });
  };
}

/**
 * The row with this id, or `null`, without saying which by taking longer.
 *
 * Mirrors `ServerActionRegistry::lookup`: every row is visited whatever
 * happens, the comparison is over the whole id, and the selected index is
 * carried in arithmetic rather than in a branch. Two rows can never share an
 * id, so the last match is the only match.
 */
function select(table: $ReadOnlyArray<ActionRecord>, id: string): ActionRecord | null {
  let selected = 0;
  let found = 0;
  for (let index = 0; index < table.length; index += 1) {
    const matches = sameId(table[index].id, id);
    const mask = -matches;
    selected = (selected & ~mask) | (index & mask);
    found |= matches;
  }
  return found === 1 ? table[selected] : null;
}

/**
 * Whether two ids are the same, in time that does not depend on how much of
 * one matched.
 *
 * The length is compared first and that is not a leak: every action id is
 * exactly 64 characters and `isActionId` has already said so about this one.
 */
function sameId(left: string, right: string): number {
  if (left.length !== right.length) {
    return 0;
  }
  let difference = 0;
  for (let index = 0; index < left.length; index += 1) {
    difference |= left.charCodeAt(index) ^ right.charCodeAt(index);
  }
  return difference === 0 ? 1 : 0;
}

/**
 * Whether the request came from the page it claims to have come from.
 *
 * Deny by default: a call with no `Origin` is refused rather than trusted,
 * because "the header was absent" and "the header matched" are not the same
 * fact and only one of them is a browser saying where it was.
 */
function sameOrigin(request: Request): boolean {
  const origin = request.headers.get("origin");
  const host = request.headers.get("host");
  if (origin == null || host == null) {
    return false;
  }
  let parsed: URL;
  try {
    // `Origin: null` — a sandboxed frame, a `data:` document, some redirects —
    // is not a URL and is refused here rather than being special-cased into a
    // value that could match something.
    parsed = new URL(origin);
  } catch {
    return false;
  }
  // `host` on both sides, so the port is part of the comparison: `:5173` and
  // `:5174` on one machine are two origins and a cookie tells them apart.
  return parsed.host === host;
}

/**
 * Whether the declared content type is the one this endpoint accepts.
 *
 * The parameters after `;` are ignored — `application/json; charset=utf-8` is
 * the same media type — and the media type itself must match exactly. A
 * `+json` suffix is not accepted: the point of the check is that a form cannot
 * produce this string, and a widened match is a widened set of things that
 * can.
 */
function isJson(declared: string | null): boolean {
  if (declared == null) {
    return false;
  }
  const semicolon = declared.indexOf(";");
  const media = (semicolon === -1 ? declared : declared.slice(0, semicolon)).trim().toLowerCase();
  return media === ACTION_CONTENT_TYPE;
}

/**
 * The body as text, or `null` when it is larger than `limit`.
 *
 * Counted as it arrives rather than trusted from `Content-Length`, because a
 * chunked request declares no length and a declared one is the sender's claim.
 * The declared length is still read first, so an oversized request that is
 * honest about it is refused before its body is transferred at all.
 *
 * `fatal: true` refuses input that is not valid UTF-8 instead of replacing
 * each bad sequence with U+FFFD, which is `docs/security.md`'s row about
 * non-UTF-8 input applied at the one place uf decodes bytes a client sent.
 */
async function readBoundedText(request: Request, limit: number): Promise<string | null> {
  const declared = request.headers.get("content-length");
  if (declared != null) {
    const size = Number(declared);
    if (!Number.isInteger(size) || size < 0 || size > limit) {
      return null;
    }
  }

  // Flow's `Request` predates a body one can read as a stream, so the property
  // is reached through a cast and the shape it is used at is checked below.
  // `ReadableStream` is not polymorphic in the DOM declarations uf checks
  // against, so the element type is asserted at the one place it is read
  // rather than written here.
  const body: ReadableStream | null = (request as $FlowFixMe).body;
  if (body == null) {
    return "";
  }

  const reader = body.getReader();
  const chunks: Array<Uint8Array> = [];
  let total = 0;
  try {
    for (;;) {
      const step = await reader.read();
      if (step.done === true) {
        break;
      }
      const chunk: Uint8Array = step.value as $FlowFixMe;
      total += chunk.byteLength;
      if (total > limit) {
        // Cancelled rather than dropped, so the sender stops instead of
        // filling a queue nobody is reading. The reason is given because the
        // declarations uf checks against require one, and "too large" is what
        // a cancelled read of an oversized body is about.
        await reader.cancel("the body is larger than a server action accepts");
        return null;
      }
      chunks.push(chunk);
    }
  } catch {
    return null;
  }

  const joined = new Uint8Array(total);
  let at = 0;
  for (const chunk of chunks) {
    joined.set(chunk, at);
    at += chunk.byteLength;
  }
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(joined);
  } catch {
    return null;
  }
}

/**
 * Every refusal, with the same body whatever it was.
 *
 * The status says what a caller may usefully do differently — retry with a
 * smaller body, send the header, look elsewhere — and the body says nothing at
 * all, because everything that could go in it is about the build.
 */
function refusal(status: number): Response {
  return new Response('{"error":"server action refused"}', {
    status,
    headers: { ...ANSWER_HEADERS },
  });
}

/**
 * Report an action that failed, where the host's error reporting can see it.
 *
 * The console, for the reason `createFetchHandler` gives about its own
 * `onError`: this runs inside a worker or a serverless invocation as often as
 * in a terminal, and losing the exception entirely is worse than putting it
 * somewhere a platform is expected to collect. The module and export are named
 * here and nowhere the caller can read.
 */
function report(record: ActionRecord, error: mixed): void {
  // eslint-disable-next-line no-console
  console.error(`uf: server action \`${record.export}\` in \`${record.module}\` failed`, error);
}
