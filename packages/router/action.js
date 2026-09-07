// @flow
//
// `@uniflowed/router/action`: a server action, as the browser holds it.
//
// A `"use client"` module writes an ordinary import and an ordinary call:
//
//   // app/counter/_components/Counter.js
//   "use client";
//   import { recordClick } from "../_actions/clicks.js";
//   …
//   <button onClick={() => { void recordClick(count); }}>
//
// On the server that import is the function. In the browser it is a reference
// built here: `@uniflowed/vite` replaces the `"use server"` module, in the
// client graph only, with one `createServerReference` per callable export, so
// what crosses the import is an id and a `fetch` — and the module's body, its
// imports and every secret they reached stay where they were written.
//
// # Why this module is where it is
//
// It is the browser's half, so it may not live in `@uniflowed/server`: that
// package is in `SERVER_ONLY_PACKAGES` and importing it from a client
// component is the error the RSC graph exists to produce. It holds no
// platform API beyond `fetch` and `location`, and it imports one internal
// module, which is the grammar the server applies to the same bytes.
//
// # The call
//
// `POST` to the page's own URL, `uf-action: <id>`, `application/json`, and
// `{"args":[…]}`. The URL is the page rather than a path uf reserves so that
// the middleware guarding that path runs above the call exactly as it does
// above the page — and it is not a substitute for the action authorizing
// itself; `internal/action-endpoint.js` says why in the paragraph that matters.
//
// # What Flow checks, and where
//
// Flow reads `app/_actions/clicks.js`, not the reference the bundler
// substitutes for it, so `recordClick("nine")` against
// `recordClick(count: number)` is a `uf check` error rather than a `400`. That
// is the half that needs nothing from this module.
//
// What a declaration alone does not say is whether those arguments and that
// result can cross a wire at all, and `ActionArguments` and `ActionResult` are
// that half: `uf prepare` writes one instantiation of each into
// `server-actions.js`, over every action in the project at once, so an action
// taking a callback or returning a `Map` is a `uf check` error naming the
// offending type. Without them the same mistake is a request that arrives with
// `{}` where an object was passed — the kind of bug found in production by a
// column that stopped being written.

import {
  ACTION_CONTENT_TYPE,
  ACTION_HEADER,
  type ActionValue,
  decodeActionResult,
  encodeActionArguments,
} from "./internal/action-wire.js";

export type { ActionValue } from "./internal/action-wire.js";
export {
  ACTION_CONTENT_TYPE,
  ACTION_HEADER,
  ActionValueError,
  MAX_ACTION_ARGUMENTS,
  MAX_ACTION_BODY_BYTES,
  MAX_ACTION_DEPTH,
  MAX_ACTION_VALUES,
} from "./internal/action-wire.js";

/**
 * A function that can be a server action.
 *
 * Both halves of the signature are the wire grammar: an action is called with
 * values that can cross and answers with one that can. `void` is a result and
 * not an argument, because JSON has no `undefined` and an action declared to
 * take one would be taking something the caller cannot send.
 */
export type ServerActionFunction = (...args: Array<ActionValue>) => Promise<ActionValue | void>;

/**
 * An action's arguments, held against what a wire can carry.
 *
 * A bound on a tuple rather than a bound on the function, and the difference
 * is the whole reason this is two types instead of one. A function type puts
 * its parameters in a contravariant position: `F extends (…args:
 * Array<ActionValue>) => …` asks whether `F` accepts *every* `ActionValue`,
 * which `createUser(name: string)` does not and should not. `Parameters<F>` is
 * the same list read covariantly, where the question is the one worth asking —
 * is each argument something that can cross?
 *
 * `uf prepare` writes one instantiation over the whole project:
 *
 *   export type ServerActionArgsFitTheWire =
 *     ActionArguments<ServerActionArgs<ServerActionName>>;
 *
 * `ServerActionName` is every action's name, so `ServerActionArgs` of it is
 * every action's argument list, and one line checks all of them.
 */
export type ActionArguments<TArgs extends $ReadOnlyArray<ActionValue>> = TArgs;

/**
 * An action's result, held against the same grammar.
 *
 * A promise, because a server action is always async — the RSC graph rejects
 * one that is not — and `void` is allowed because an action that returns
 * nothing is ordinary. `Promise` is covariant in Flow, so the bound reaches
 * the resolved type without any of the contortion the arguments needed.
 */
export type ActionResult<TResult extends Promise<ActionValue | void>> = TResult;

/**
 * A server action that did not answer.
 *
 * Carries the status and the action's build-time name and nothing else,
 * because nothing else came back: the endpoint answers every refusal with a
 * fixed body, so there is no message from the server to relay. What went wrong
 * is in the server's log, which is where an application's internals belong.
 */
export class ServerActionError extends Error {
  /** The HTTP status the endpoint answered with. */
  status: number;
  /** `module#export` of the action that was called. */
  action: string;

  constructor(action: string, status: number) {
    super(
      `@uniflowed/router: the server action \`${action}\` answered ${String(status)}. ` +
        "The endpoint reports every failure the same way; the reason is in the server's log.",
    );
    this.name = "ServerActionError";
    this.status = status;
    this.action = action;
  }
}

/**
 * The reference the client bundle holds in place of one server action.
 *
 * Generated, never written by hand: `@uniflowed/vite` emits one call per
 * callable export of a `"use server"` module, with the id
 * `crates/uf_rsc/src/action.rs` derived for it and the `module#export` name
 * that only ever appears in an error.
 *
 * The returned function is `async` and refuses before it sends: an argument
 * outside the wire grammar throws an `ActionValueError` naming the argument's
 * position, at the call site, rather than becoming a `400` with nothing in it.
 */
export function createServerReference(id: string, name: string): ServerActionFunction {
  return async function callServerAction(...args: Array<ActionValue>): Promise<ActionValue | void> {
    const body = encodeActionArguments(args);
    // Built rather than written as a literal, because the header's name is a
    // constant and a computed key in an object literal is a shape Flow
    // declines to track.
    const headers: { [string]: string } = { "content-type": ACTION_CONTENT_TYPE };
    headers[ACTION_HEADER] = id;
    const response = await fetch(currentUrl(), {
      method: "POST",
      // Stated rather than left to the default, because the default is what a
      // reader has to look up and because this one is load-bearing: the
      // endpoint's `Origin` check is only meaningful for a request that
      // carries the visitor's cookies in the first place.
      credentials: "same-origin",
      // Never a cached answer, and never one written to a cache: an action is
      // a side effect, and `POST` responses are outside HTTP caching by
      // default only until something decides otherwise.
      cache: "no-store",
      headers,
      body,
    });
    if (!response.ok) {
      throw new ServerActionError(name, response.status);
    }
    return decodeActionResult(await response.text());
  };
}

/**
 * The URL an action call is sent to: the page the browser is on.
 *
 * Path and query, not the whole URL, so the request is same-origin by
 * construction rather than by a comparison somebody could get wrong. The hash
 * is left off because it never reaches a server.
 *
 * A reference only exists in the client bundle — the server imports the real
 * module — so there is no `location` here only if something has imported the
 * browser's half into a server, and saying so is better than posting to a
 * relative path that means nothing there.
 */
function currentUrl(): string {
  const location = globalThis.location;
  if (location == null) {
    throw new Error(
      "@uniflowed/router: a server action reference was called where there is no `location`. " +
        "A reference is the browser's half of an action; the server imports the module itself.",
    );
  }
  return `${location.pathname}${location.search}`;
}
