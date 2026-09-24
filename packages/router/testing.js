// @flow
//
// `@uniflowed/router/testing`: Server Components and server actions, under test.
//
// Three levels, and each one is the real code path at that level rather than a
// second implementation of it that a test could pass against while production
// failed:
//
//   1. **A request scope** — [`withRequest`]. A server function, a data
//      function or an async Server Component called as a function reads
//      `cookies()`, `headers()` and `after()` from the request it is inside.
//      This is the request lifecycle every host begins (`beginRequest`), with a
//      `Request` the test describes, settled when the body returns so that an
//      `after()` callback has run by the time the test asserts.
//   2. **A server action through its endpoint** — [`callAction`] and
//      [`serverReferences`]. The arguments are encoded by the encoder the
//      browser's reference uses, posted to the dispatcher every host runs, and
//      the answer decoded by the decoder the browser uses. So an argument that
//      cannot cross fails at the call site exactly as it does in a browser, an
//      action that throws is the fixed `500` a browser gets, and what came back
//      is what JSON carried rather than the object the function returned.
//   3. **A production build, in process** — [`buildApp`] and [`openBuild`].
//      `uf build --adapter node` writes the application as a `Request` →
//      `Response` function (`handler.js`); this loads it and answers requests
//      the way `@uniflowed/server/node` does — the build's files first, then
//      the application, inside the handler's own request lifecycle — with no
//      socket and no port. HTML documents, Flight payloads, route handlers,
//      redirects, error and not-found boundaries, the route cache and a form
//      posted before hydration all come out of the build's real renderer.
//
// # Why these live in the router
//
// Each helper drives something only this package has: the action wire
// (`./internal/action-wire.js`), the action dispatcher
// (`./internal/action-endpoint.js`) and the routing errors a Server Component
// throws (`./internal/routing.js`). Written anywhere else they would either
// reach into this package's internals by path, or copy them — and a copy of
// the wire grammar is exactly the thing that agrees with the real one until it
// does not. The router is also published, so a project can use these today.
//
// # What is deliberately not here
//
// - **Rendering one Server Component to Flight in the test's own process.**
//   React's Flight renderer only loads where `react` resolved under the
//   `react-server` condition, and a test worker resolves the other React for
//   everything else it runs. A render that faked the boundary — calling an
//   async component and rendering its element with `react-dom/server` — would
//   let a function prop cross into a client component, which Flight refuses,
//   and would pass. [`openBuild`] renders through the real graphs instead, and
//   `createTestApp` in `@uniflowed/test/app` does the same through `uf dev`.
// - **A mock of the endpoint.** `callAction` runs the dispatcher itself. What
//   it substitutes is the manifest's id — a test calls a function it imported,
//   not an id a build minted — and nothing else.
// - **Following a redirect.** Every answer is returned as the endpoint or the
//   build wrote it, `303` and `Location` included, so a test can assert on it.

import { type CacheOptions, CacheStore } from "@uniflowed/server/cache";
import { beginRequest } from "@uniflowed/server/host";

import { ServerActionError } from "./action.js";
import { createActionDispatcher } from "./internal/action-endpoint.js";
import {
  ACTION_CONTENT_TYPE,
  ACTION_HEADER,
  type ActionArgument,
  ActionValueError,
  decodeActionResult,
  encodeActionArguments,
  encodeActionResult,
} from "./internal/action-wire.js";
import {
  FORM_ACTION_CONTENT_TYPE,
  FORM_BOUND_FIELD,
  FORM_ID_FIELD,
  FORM_REF_FIELD,
  withFormAction,
} from "./internal/form-action.js";
import {
  ForbiddenError,
  NotFoundError,
  RedirectError,
  UnauthorizedError,
} from "./internal/routing.js";

/**
 * The origin a test request is addressed to when it names only a path.
 *
 * `localhost` rather than a real name, so nothing a test writes can be mistaken
 * for a request to somebody's site, and plain `http` because nothing here opens
 * a connection.
 */
export const TEST_ORIGIN = "http://localhost";

/**
 * The id every action `callAction` calls is filed under.
 *
 * A real id is `HMAC-SHA256(build id, module ‖ export ‖ kind)` and only a build
 * can mint one. A test holds the function rather than an id, so it is filed in
 * a one-row table under this canonical 64-hex-digit id, and the dispatcher's
 * lookup, comparison and refusals run unchanged. It reads as `test action` in
 * hexspeak so that it is recognisable in a log line.
 */
export const TEST_ACTION_ID: string = "7e57ac7107e57ac7".repeat(4);

/**
 * The request a test describes.
 *
 * Every field is optional; the empty description is a `GET` of
 * `http://localhost/` with no cookies. `cookies` is written into the `Cookie`
 * header (after any `cookie` in `headers`), so a test says which session it is
 * without spelling the header.
 */
export type TestRequestInit = {|
  /** A path on [`TEST_ORIGIN`], or an absolute URL. Defaults to `/`. */
  readonly url?: string,
  readonly method?: string,
  readonly headers?: { readonly [string]: string },
  readonly cookies?: { readonly [string]: string },
  readonly body?: string | URLSearchParams | FormData,
  /**
   * The cache the host would install for this request, which is what
   * `revalidateTag`, `revalidatePath` and `cacheFunction` reach.
   *
   * Absent means none, which is what a project without `app.rendering.cache`
   * gets — so an action that calls `revalidateTag` there fails here as it
   * fails there. A bare `CacheStore` is installed with `route`, `fetch` and
   * `data` all on.
   */
  readonly cache?: CacheStore | CacheOptions,
|};

/** What a request did to the cache it was answered with. */
export type Revalidations = {|
  /** Every tag `revalidateTag` or `updateTag` expired, in call order. */
  readonly tags: $ReadOnlyArray<string>,
  /** Every path `revalidatePath` expired, in call order. */
  readonly paths: $ReadOnlyArray<string>,
|};

/**
 * Build the `Request` a [`TestRequestInit`] describes.
 *
 * Exported because a route handler or a middleware under test takes a
 * `Request`, and building one by hand is where a missing `Host` or a
 * misspelled cookie header comes from. `Host` is always set to the URL's own,
 * since that is what every uf host compares `Origin` against.
 */
export function testRequest(init?: TestRequestInit): Request {
  const url = new URL(init?.url ?? "/", TEST_ORIGIN);
  const headers = new Headers();
  const named: { readonly [string]: string } = init?.headers ?? {};
  for (const name of Object.keys(named)) {
    headers.set(name, named[name]);
  }
  if (!headers.has("host")) {
    headers.set("host", url.host);
  }
  const cookies = init?.cookies;
  if (cookies != null) {
    const pairs = Object.entries(cookies).map(
      ([name, value]) => `${name}=${encodeURIComponent(String(value))}`,
    );
    const given = headers.get("cookie");
    headers.set("cookie", [given, ...pairs].filter(Boolean).join("; "));
  }
  const method = (init?.method ?? "GET").toUpperCase();
  return new Request(url.href, {
    method,
    headers,
    body: method === "GET" || method === "HEAD" ? undefined : init?.body,
  });
}

/**
 * Run `body` inside a request, as a host would, and settle the request after.
 *
 * Inside, `cookies()`, `headers()`, `requestId()`, `draftMode()` and `after()`
 * answer about `request`; `revalidateTag` and `cacheFunction` reach `cache`
 * when one is given. When `body` has finished — returned, resolved or thrown —
 * the request is settled, which is when a host runs what `after()` deferred,
 * so a test can assert on an `after()` callback's effect as soon as this
 * resolves.
 *
 * `body`'s value or rejection is passed through unchanged. That is the point
 * for a Server Component: `notFound()` inside one rejects with the router's
 * `NotFoundError`, which a test asserts with `rejects.toBeInstanceOf`.
 *
 * Two calls are two requests, however they interleave: the scope is the
 * asynchronous call tree, which is what makes a test of request isolation
 * meaningful here.
 */
export async function withRequest<T>(
  request: Request | TestRequestInit,
  body: () => T | Promise<T>,
): Promise<T> {
  const cache = request instanceof Request ? undefined : request.cache;
  const built = request instanceof Request ? request : testRequest(request);
  return (await withLifecycle(built, cache, async () => body())).value;
}

/** How `callAction` sends a call: the way a hydrated page does, or a native form post. */
export type ActionDoor = "fetch" | "form";

/** What a test says about one call to a server action. */
export type ActionCallInit = {|
  /**
   * The page the call is made from — the URL the browser posts to. Defaults
   * to `/`. A path guard's middleware is not run: an action authorizes
   * itself, which is what a test of one should prove.
   */
  readonly url?: string,
  /** Headers beyond the ones the browser's reference sends. Override to test a refusal. */
  readonly headers?: { readonly [string]: string },
  readonly cookies?: { readonly [string]: string },
  readonly cache?: CacheStore | CacheOptions,
  /**
   * Which door the call comes through.
   *
   * `"fetch"`, the default, is a hydrated page: `POST`, `uf-action`, JSON.
   * `"form"` is a form submitted before the page hydrated: a urlencoded post
   * whose last argument is the form, answered with a `303` rather than JSON.
   * The two answer a `redirect()` differently, and that is the reason to be
   * able to ask for either.
   */
  readonly door?: ActionDoor,
  /** `module#export`, for the `ServerActionError` a failed reference throws. */
  readonly name?: string,
|};

/**
 * What one call to a server action came to.
 *
 * `kind` is what the *action* did. `response` is what the endpoint answered,
 * which is what a browser gets, and the two doors answer the same kind
 * differently: `redirect()` is a `204` carrying `Location` and the
 * `uf-action-outcome` header through `"fetch"`, and a `303` through `"form"`;
 * `notFound()`, `unauthorized()` and `forbidden()` are their page statuses
 * through `"fetch"`. Assert on both when both matter.
 *
 * Every outcome has been settled — `after()` callbacks have run — and carries
 * what the call did to the cache in `revalidated`.
 */
export type ActionOutcome<R> =
  | {|
      readonly kind: "returned",
      /**
       * Through the `"fetch"` door, the value as the browser decodes it from
       * the answer — so `undefined` inside an object is gone and an object is
       * a new one. Through `"form"`, the value the action returned, which a
       * browser never sees.
       */
      readonly value: R,
      readonly response: Response,
      readonly revalidated: Revalidations,
    |}
  | {|
      readonly kind: "redirect",
      readonly to: string,
      readonly permanent: boolean,
      readonly response: Response,
      readonly revalidated: Revalidations,
    |}
  | {|
      readonly kind: "not-found",
      readonly response: Response,
      readonly revalidated: Revalidations,
    |}
  | {|
      readonly kind: "unauthorized",
      readonly response: Response,
      readonly revalidated: Revalidations,
    |}
  | {|
      readonly kind: "forbidden",
      readonly response: Response,
      readonly revalidated: Revalidations,
    |}
  | {|
      readonly kind: "threw",
      readonly error: mixed,
      readonly response: Response,
      readonly revalidated: Revalidations,
    |}
  | {|
      /**
       * The action returned a value the wire cannot carry back — a `Date`, a
       * `Map`, a class instance. `error` is the `ActionValueError` naming
       * where it was; the browser got a `500`.
       */
      readonly kind: "unsendable",
      readonly error: ActionValueError,
      readonly response: Response,
      readonly revalidated: Revalidations,
    |}
  | {|
      /**
       * The endpoint refused the request before running anything: a guard
       * the test's `headers` overrode, or a payload over a limit only the
       * decoder counts. `response.status` says which.
       */
      readonly kind: "refused",
      readonly response: Response,
      readonly revalidated: Revalidations,
    |};

/**
 * Call a server action the way a browser does, through the endpoint that
 * answers one, and say what happened.
 *
 * The arguments are encoded by the browser's own encoder first, so an argument
 * that cannot cross — a `Date`, a function, a `File`, a cycle — rejects this
 * call with an `ActionValueError` before anything is sent, which is where a
 * browser throws it too. Then the call is posted to the dispatcher every uf
 * host runs, inside a request with the test's cookies and headers and the
 * page's own `Origin` and `Host`, and the answer is classified into an
 * [`ActionOutcome`].
 *
 * The function is filed under [`TEST_ACTION_ID`] in a table of one, which is
 * the only thing substituted. Middleware is not run: the endpoint runs below
 * it in every host, and an action that relies on a path guard is an action
 * with a hole in it (see the server actions guide).
 *
 * Through the `"form"` door the last argument must be the `FormData`, and the
 * ones before it are the bound arguments a form carries in its hidden fields
 * — `useActionState`'s previous state, for one. That door answers with a
 * `303`; the page rendered again with a `useActionState` result needs the
 * whole application, which is [`openBuild`]'s `submit`.
 */
export async function callAction<Args extends $ReadOnlyArray<ActionArgument>, R>(
  action: (...args: Args) => Promise<R>,
  args: Args,
  init?: ActionCallInit,
): Promise<ActionOutcome<R>> {
  const door = init?.door ?? "fetch";
  const request = door === "form" ? formActionRequest(args, init) : fetchActionRequest(args, init);

  // What the function itself did, seen before the endpoint turns it into a
  // status. The endpoint answers every failure identically on purpose; a test
  // is the one caller entitled to know which failure it was.
  let seen:
    | {| readonly returned: true, readonly value: mixed |}
    | {| readonly returned: false, readonly error: mixed |}
    | null = null;
  const observed = async (...received: Args): Promise<R> => {
    try {
      const value = await action(...received);
      seen = { returned: true, value };
      return value;
    } catch (error) {
      seen = { returned: false, error };
      throw error;
    }
  };
  const dispatch = createActionDispatcher({
    actions: [
      {
        id: TEST_ACTION_ID,
        module: init?.name ?? "server action under test",
        export: "action",
        load: async () => ({ action: observed }),
      },
    ],
  });

  const answered = await withLifecycle(request, init?.cache, () => dispatch(request));
  const { revalidated } = answered;
  const response = answered.value;
  if (response == null) {
    // The dispatcher declines only a request that names no action, and both
    // requests built above name one. Reaching this is this module's bug.
    throw new Error("@uniflowed/router/testing: the action endpoint declined its own request");
  }

  const outcome = seen;
  if (outcome == null) {
    return { kind: "refused", response, revalidated };
  }
  if (outcome.returned === false) {
    const error = outcome.error;
    if (error instanceof RedirectError) {
      return { kind: "redirect", to: error.to, permanent: error.permanent, response, revalidated };
    }
    if (error instanceof NotFoundError) return { kind: "not-found", response, revalidated };
    if (error instanceof UnauthorizedError) return { kind: "unauthorized", response, revalidated };
    if (error instanceof ForbiddenError) return { kind: "forbidden", response, revalidated };
    return { kind: "threw", error, response, revalidated };
  }
  if (door === "fetch") {
    if (response.status !== 200) {
      // It returned and the endpoint still answered 500: the result could not
      // be encoded. Encode it again for the test, to hand back the reason.
      return { kind: "unsendable", error: unsendable(outcome.value), response, revalidated };
    }
    // The decoded answer is structurally the `R` the action returned: the
    // endpoint held that value to the grammar before encoding it, and the
    // decoder checked it again. The cast is the one place that says so.
    const value: R = decodeActionResult(await response.clone().text()) as $FlowFixMe;
    return { kind: "returned", value, response, revalidated };
  }
  if (response.status === 500) {
    return { kind: "unsendable", error: unsendable(outcome.value), response, revalidated };
  }
  const value: R = outcome.value as $FlowFixMe;
  return { kind: "returned", value, response, revalidated };
}

/**
 * Stand-ins for a `"use server"` module's exports that call through the wire.
 *
 * For a client component under test. `uf test` imports a `"use server"`
 * module as the module itself — there is no bundler replacing it with
 * references — so a component that calls one would otherwise call the
 * function directly, outside any request, with arguments nothing encoded.
 * Handing these to `uft.mock` instead makes each call what it is in a
 * browser: encoded, posted to the endpoint, run inside a request carrying
 * `init`'s cookies, and decoded. A routing call is followed as the browser's
 * reference follows it (a redirect resolves with nothing; `notFound()`,
 * `unauthorized()` and `forbidden()` reject with the router's error), and any
 * other failure rejects with the `ServerActionError` a browser's reference throws.
 *
 * `init` may be a function, read on every call, so a test can change who is
 * signed in between two calls. Every non-function export is passed through.
 */
export function serverReferences<M extends { readonly [string]: mixed }>(
  module: M,
  init?: ActionCallInit | (() => ActionCallInit),
): M {
  const references: { [string]: mixed } = {};
  for (const [name, exported] of Object.entries(module)) {
    if (typeof exported !== "function") {
      references[name] = exported;
      continue;
    }
    // A `"use server"` export is `async (...ActionArgument) => …` by the RSC
    // graph's contract; `Object.entries` cannot carry that type, so it is
    // restated for the one call below.
    const action: (...args: $ReadOnlyArray<ActionArgument>) => Promise<mixed> =
      exported as $FlowFixMe;
    const reference = async (...args: $ReadOnlyArray<ActionArgument>): Promise<mixed> => {
      const given: ActionCallInit = (typeof init === "function" ? init() : init) ?? {};
      const label = given.name ?? name;
      const outcome = await callAction(action, args, {
        url: given.url,
        headers: given.headers,
        cookies: given.cookies,
        cache: given.cache,
        name: label,
        door: "fetch",
      });
      // What the browser's reference does with each outcome: a redirect
      // resolves with nothing once it has navigated (there is no navigation
      // here, so it resolves at once), and the other three routing calls
      // reject with the error the server caught, for the route's boundary.
      if (outcome.kind === "returned") {
        return outcome.value;
      }
      if (outcome.kind === "redirect") {
        return undefined;
      }
      if (outcome.kind === "not-found") throw new NotFoundError();
      if (outcome.kind === "unauthorized") throw new UnauthorizedError();
      if (outcome.kind === "forbidden") throw new ForbiddenError();
      throw new ServerActionError(label, outcome.response.status);
    };
    // The same `$$FORM_ACTION` the browser's reference carries, so a form
    // bound to one renders as the real form would.
    references[name] = withFormAction(reference, TEST_ACTION_ID, []);
  }
  // Same keys, and every function replaced by one with its signature: `M`.
  return references as $FlowFixMe;
}

/** A request to the JSON door, exactly as `createServerReference` sends one. */
function fetchActionRequest(args: $ReadOnlyArray<ActionArgument>, init?: ActionCallInit): Request {
  const body = encodeActionArguments(args);
  const url = new URL(init?.url ?? "/", TEST_ORIGIN);
  const headers = merged(
    { origin: url.origin, "content-type": ACTION_CONTENT_TYPE },
    { [ACTION_HEADER]: TEST_ACTION_ID },
    init?.headers,
  );
  return testRequest({
    url: url.href,
    method: "POST",
    headers,
    cookies: init?.cookies,
    body,
  });
}

/**
 * A request to the native-form door, exactly as a browser submits the form
 * React wrote from `$$FORM_ACTION` before the page hydrated.
 */
function formActionRequest(args: $ReadOnlyArray<ActionArgument>, init?: ActionCallInit): Request {
  const form = args[args.length - 1];
  if (!(form instanceof FormData)) {
    throw new TypeError(
      '@uniflowed/router/testing: a call through the "form" door ends with the FormData ' +
        "the form submits; the arguments before it are the ones the form carries bound.",
    );
  }
  const fields = new URLSearchParams();
  fields.append(`${FORM_REF_FIELD}0`, "");
  fields.append(`${FORM_ID_FIELD}0`, TEST_ACTION_ID);
  const bound = args.slice(0, -1);
  if (bound.length > 0) {
    fields.append(`${FORM_BOUND_FIELD}0`, encodeActionArguments(bound));
  }
  for (const [name, value] of form.entries()) {
    if (typeof value !== "string") {
      // What the browser-side encoder says about a file, said at the same place.
      throw new ActionValueError(`the form field ${JSON.stringify(name)}`, "is a file");
    }
    fields.append(name, value);
  }
  const url = new URL(init?.url ?? "/", TEST_ORIGIN);
  return testRequest({
    url: url.href,
    method: "POST",
    headers: merged(
      {
        origin: url.origin,
        "sec-fetch-site": "same-origin",
        "content-type": FORM_ACTION_CONTENT_TYPE,
      },
      init?.headers,
    ),
    cookies: init?.cookies,
    body: fields.toString(),
  });
}

/**
 * Header maps combined left to right, a later one winning a name.
 *
 * A loop rather than a spread, because Flow cannot say what spreading two
 * indexed objects produces, and the order is the contract: the test's own
 * `headers` come last so that a test can override a guard on purpose.
 */
function merged(...maps: $ReadOnlyArray<?{ readonly [string]: string }>): {
  readonly [string]: string,
} {
  const out: { [string]: string } = {};
  for (const map of maps) {
    const entries: { readonly [string]: string } = map ?? {};
    for (const name of Object.keys(entries)) {
      out[name.toLowerCase()] = entries[name];
    }
  }
  return out;
}

/** Why a value an action returned could not be its answer. */
function unsendable(value: mixed): ActionValueError {
  try {
    encodeActionResult(value);
  } catch (error) {
    if (error instanceof ActionValueError) {
      return error;
    }
  }
  return new ActionValueError("the result", "could not be encoded");
}

/**
 * One request through `beginRequest`, with the test's cache installed and
 * watched, settled when `body` is done.
 *
 * The cache is installed on the context before `run`, which is the moment
 * `createFetchHandler` installs a host's — before the guard and the dispatcher,
 * so a mutation reaches the store that answered the request.
 */
async function withLifecycle<T>(
  request: Request,
  cache: CacheStore | CacheOptions | void,
  body: () => Promise<T>,
): Promise<{| readonly value: T, readonly revalidated: Revalidations |}> {
  const lifecycle = beginRequest(request);
  const tags: Array<string> = [];
  const paths: Array<string> = [];
  const installed: CacheOptions | null =
    cache == null
      ? null
      : cache instanceof CacheStore
        ? { store: cache, route: true, fetch: true, data: true }
        : cache;
  const restore = installed == null ? () => {} : watchInvalidations(installed.store, tags, paths);
  lifecycle.context.cache = installed;
  try {
    const value = await lifecycle.run(body);
    return { value, revalidated: { tags, paths } };
  } finally {
    await lifecycle.settle();
    restore();
  }
}

/**
 * Record every tag and path `store` is asked to expire until the returned
 * function is called.
 *
 * On the instance, and put back afterwards, because the store is the test's
 * own object and the only other way to see an invalidation — its statistics —
 * counts them without naming them.
 */
function watchInvalidations(
  store: CacheStore,
  tags: Array<string>,
  paths: Array<string>,
): () => void {
  // Taken off the instance and called with it below, which is the binding the
  // lint rule is protecting; the method never runs unbound.
  // $FlowFixMe[method-unbinding]
  const byTag = store.revalidateTag;
  // $FlowFixMe[method-unbinding]
  const byPath = store.revalidatePath;
  // $FlowFixMe[cannot-write] the instance's own property shadows the method for this request only.
  store.revalidateTag = (tag: string): number => {
    tags.push(tag);
    return byTag.call(store, tag);
  };
  // $FlowFixMe[cannot-write] as above.
  store.revalidatePath = (path: string): number => {
    paths.push(path);
    return byPath.call(store, path);
  };
  return () => {
    // $FlowFixMe[cannot-write] removing the shadow puts the prototype's method back.
    delete store.revalidateTag;
    // $FlowFixMe[cannot-write] as above.
    delete store.revalidatePath;
  };
}

// ---------------------------------------------------------------------------
// A production build, in process
// ---------------------------------------------------------------------------

/** A request to a built application, described by what a test cares about. */
export type BuiltRequestInit = {|
  readonly method?: string,
  readonly headers?: { readonly [string]: string },
  readonly cookies?: { readonly [string]: string },
  readonly body?: string | URLSearchParams | FormData,
|};

/**
 * A built application, answering requests in the test's own process.
 *
 * Every method takes a path on the application's origin and returns the
 * `Response` the build wrote, unfollowed and still streaming.
 */
export type BuiltApp = {|
  /** The directory `handler.js` was loaded from. */
  readonly directory: string,
  /** Any request: a route handler, a static file, a `POST`. */
  readonly fetch: (pathname: string, init?: BuiltRequestInit) => Promise<Response>,
  /** A document, asked for the way a browser navigating to it asks. */
  readonly render: (pathname: string, init?: BuiltRequestInit) => Promise<Response>,
  /**
   * The route's React Flight payload, as a client navigation fetches it.
   * Rejects when the answer is not a Flight stream, with the status and the
   * start of what came back instead.
   */
  readonly flight: (pathname: string, init?: BuiltRequestInit) => Promise<Response>,
  /**
   * Submit a form on the page at `pathname` before it hydrates.
   *
   * Renders the page, reads the `index`th `<form>`'s hidden fields — the ones
   * React wrote for a server action: its build id, its bound arguments, the
   * `useActionState` key — and posts them with `fields` as a browser without
   * JavaScript would. So the action is dialled by the id the build minted,
   * through the endpoint's second door, and a `useActionState` form comes back
   * as the page rendered with the action's result.
   */
  readonly submit: (
    pathname: string,
    fields: { readonly [string]: string },
    options?: {| readonly form?: number, readonly cookies?: { readonly [string]: string } |},
  ) => Promise<Response>,
  /** Resolves once every request answered so far has settled (`after()` has run). */
  readonly settled: () => Promise<void>,
|};

/** A loaded `handler.js`, checked field by field. */
type Handler = {|
  readonly fetch: (request: Request) => Promise<Response>,
  readonly beginRequest: (request: Request) => {|
    readonly run: <T>(body: () => Promise<T>) => Promise<T>,
    readonly settle: () => Promise<void>,
  |},
  readonly routing?: mixed,
|};

/**
 * Open the application `uf build --adapter node` wrote to `directory`.
 *
 * `directory` holds `handler.js` and `static/` — `.uf/deploy/node` under the
 * project. Requests are answered as `@uniflowed/server/node` answers them:
 * `app.router`'s redirects and headers, then a file under `static/` for a
 * `GET` or `HEAD`, then the application, all inside the lifecycle the
 * handler's own `beginRequest` begins. A request is settled when its body has
 * been read to the end or cancelled, which is when a Node host settles one.
 *
 * Nothing listens. That is what makes it usable where a test may not open a
 * socket, and why two opened builds cannot collide on a port.
 */
export async function openBuild(directory: string | URL): Promise<BuiltApp> {
  const { fileURLToPath, pathToFileURL } = await import("node:url");
  const path = await import("node:path");
  const root = path.resolve(directory instanceof URL ? fileURLToPath(directory.href) : directory);
  // The build's own module, whose path is only known at run time; Flow types
  // only a literal specifier, and `asHandler` checks what came back.
  // $FlowFixMe[unsupported-syntax]
  const loaded: mixed = await import(pathToFileURL(path.join(root, "handler.js")).href);
  const handler = asHandler(loaded, root);
  const { createServeHandler } = await import("@uniflowed/server/node");
  const serve = createServeHandler({
    staticDir: path.join(root, "static"),
    handle: handler.fetch,
    // The build's own rules, whatever their shape; `createServeHandler` reads them.
    routing: handler.routing as $FlowFixMe,
  });
  const pending: Set<Promise<void>> = new Set();

  async function answer(request: Request): Promise<Response> {
    const lifecycle = handler.beginRequest(request);
    let response: Response;
    try {
      response = await lifecycle.run(() => serve(request));
    } catch {
      // Obligation 6 of the adapter contract: a bare 500, nothing of the error.
      response = new Response("500 Internal Server Error\n", {
        status: 500,
        headers: { "content-type": "text/plain; charset=utf-8" },
      });
    }
    let settle: () => void = () => {};
    const done = new Promise<void>((resolve) => {
      settle = () => {
        lifecycle.settle().then(resolve, resolve);
      };
    });
    pending.add(done);
    void done.then(() => pending.delete(done));
    const body = response.body;
    if (body == null) {
      settle();
      return response;
    }
    // Settle when the test has read the body, or given up on it: the moment a
    // Node host's response `finish`es. Earlier would run `after()` while a
    // Suspense boundary is still streaming.
    const reader = body.getReader();
    const watched = new ReadableStream<Uint8Array>({
      async pull(controller) {
        const next = await reader.read();
        if (next.done === true) {
          controller.close();
          settle();
        } else {
          controller.enqueue(next.value);
        }
      },
      async cancel(reason) {
        await reader.cancel(reason);
        settle();
      },
    });
    return new Response(watched, {
      status: response.status,
      statusText: response.statusText,
      headers: response.headers,
    });
  }

  function request(
    pathname: string,
    init?: BuiltRequestInit,
    extra?: { readonly [string]: string },
  ): Request {
    if (!pathname.startsWith("/") || pathname.startsWith("//")) {
      throw new TypeError(
        "@uniflowed/router/testing: a built app is asked for a path, like `/notes`",
      );
    }
    return testRequest({
      url: pathname,
      method: init?.method,
      headers: merged(extra, init?.headers),
      cookies: init?.cookies,
      body: init?.body,
    });
  }

  return {
    directory: root,
    fetch: (pathname, init) => answer(request(pathname, init)),
    render: (pathname, init) => answer(request(pathname, init, { accept: "text/html" })),
    flight: async (pathname, init) => {
      const url = new URL(pathname, TEST_ORIGIN);
      url.pathname = `${url.pathname.replace(/\/$/, "")}/__uf.flight`;
      const response = await answer(request(`${url.pathname}${url.search}`, init));
      if (!(response.headers.get("content-type") ?? "").includes("text/x-component")) {
        const text = (await response.text()).slice(0, 200);
        throw new Error(
          `@uniflowed/router/testing: ${pathname} answered ${String(response.status)} ` +
            `without a Flight payload: ${text}`,
        );
      }
      return response;
    },
    submit: async (pathname, fields, options) => {
      const page = await answer(
        request(pathname, { cookies: options?.cookies }, { accept: "text/html" }),
      );
      const html = await page.text();
      const form = formsIn(html)[options?.form ?? 0];
      if (form == null) {
        throw new Error(
          `@uniflowed/router/testing: ${pathname} has no form number ${String(options?.form ?? 0)}`,
        );
      }
      const body = new URLSearchParams();
      for (const [name, value] of form.hidden) body.append(name, value);
      for (const [name, value] of Object.entries(fields)) body.append(name, value);
      const target = new URL(form.action === "" ? pathname : form.action, TEST_ORIGIN);
      return answer(
        testRequest({
          url: `${target.pathname}${target.search}`,
          method: "POST",
          headers: {
            origin: TEST_ORIGIN,
            "sec-fetch-site": "same-origin",
            "content-type": FORM_ACTION_CONTENT_TYPE,
          },
          cookies: options?.cookies,
          body: body.toString(),
        }),
      );
    },
    settled: async () => {
      while (pending.size > 0) {
        await Promise.all(Array.from(pending));
      }
    },
  };
}

/** Options for [`buildApp`]. */
export type BuildAppOptions = {|
  /** The directory holding `uf.config.js`. */
  readonly root: string | URL,
  /** The `uf` to run. Defaults to `UF_BINARY`, which `uf test` sets, then `uf`. */
  readonly binary?: string,
  /** Added to the build's environment, e.g. `UF_BUILD_ID` for reproducible action ids. */
  readonly env?: { readonly [string]: string },
|};

/**
 * Build the project at `root` for production and [`openBuild`] the result.
 *
 * Runs `uf build --adapter node`, which writes `.uf/deploy/node` under the
 * project, and rejects with the build's own output when it fails — an RSC
 * contract violation is a build failure, and the test should say which one.
 *
 * A build is seconds, not milliseconds: call this once, in `beforeAll`, and
 * share the app across the file. Two files building the same project at once
 * would write the same directory, so give the integration tests of one
 * project one file.
 */
export async function buildApp(options: BuildAppOptions): Promise<BuiltApp> {
  const { spawn } = await import("node:child_process");
  const { fileURLToPath } = await import("node:url");
  const path = await import("node:path");
  const root = path.resolve(
    options.root instanceof URL ? fileURLToPath(options.root.href) : options.root,
  );
  const binary = options.binary ?? process.env.UF_BINARY ?? "uf";
  const child = spawn(binary, ["--cwd", root, "build", "--adapter", "node", "--color", "never"], {
    env: { ...process.env, ...options.env },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let transcript = "";
  const keep = (chunk: mixed) => {
    transcript = (transcript + String(chunk)).slice(-16384);
  };
  child.stdout?.on("data", keep);
  child.stderr?.on("data", keep);
  const code = await new Promise<number | null>((resolve, reject) => {
    child.once("error", reject);
    child.once("close", resolve);
  });
  if (code !== 0) {
    throw new Error(
      `@uniflowed/router/testing: \`uf build\` exited ${String(code)}\n${transcript}`,
    );
  }
  return openBuild(path.join(root, ".uf", "deploy", "node"));
}

/** `loaded` as a handler, or an error naming what it is missing. */
function asHandler(loaded: mixed, root: string): Handler {
  const candidate =
    loaded != null && typeof loaded === "object" && loaded.default != null && loaded.fetch == null
      ? loaded.default
      : loaded;
  if (
    candidate == null ||
    typeof candidate !== "object" ||
    typeof candidate.fetch !== "function" ||
    typeof candidate.beginRequest !== "function"
  ) {
    throw new TypeError(
      `@uniflowed/router/testing: ${root}/handler.js is not a uf handler (it needs \`fetch\` and ` +
        "`beginRequest`). Open the directory `uf build --adapter node` wrote, `.uf/deploy/node`.",
    );
  }
  // Both functions were checked above; `mixed` cannot carry that.
  return candidate as $FlowFixMe;
}

/** One `<form>` in a document: where it posts, and the hidden fields React wrote. */
type FormFields = {| readonly action: string, readonly hidden: $ReadOnlyArray<[string, string]> |};

/**
 * Every `<form>` in `html`, in document order, with its hidden inputs.
 *
 * A reader for the markup React writes and for nothing more: attributes in
 * double quotes, the five entities `react-dom/server` escapes. That is enough
 * because the document is the build's own; it is not an HTML parser and is not
 * offered as one.
 */
export function formsIn(html: string): Array<FormFields> {
  const forms: Array<FormFields> = [];
  const formPattern = /<form\b([^>]*)>([\s\S]*?)<\/form>/g;
  for (const match of html.matchAll(formPattern)) {
    const attributes = attributesOf(match[1]);
    const hidden: Array<[string, string]> = [];
    for (const input of match[2].matchAll(/<input\b([^>]*?)\/?>/g)) {
      const fields = attributesOf(input[1]);
      if (fields.get("type") === "hidden" && fields.has("name")) {
        hidden.push([fields.get("name") ?? "", fields.get("value") ?? ""]);
      }
    }
    forms.push({ action: attributes.get("action") ?? "", hidden });
  }
  return forms;
}

function attributesOf(source: string): Map<string, string> {
  const found = new Map<string, string>();
  for (const attribute of source.matchAll(/([^\s=/]+)(?:="([^"]*)")?/g)) {
    found.set(attribute[1].toLowerCase(), unescapeHtml(attribute[2] ?? ""));
  }
  return found;
}

function unescapeHtml(text: string): string {
  return text
    .replace(/&quot;/g, '"')
    .replace(/&#x27;/g, "'")
    .replace(/&#39;/g, "'")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&");
}
