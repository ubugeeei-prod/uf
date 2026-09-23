// @flow
//
// Internal to `@uniflowed/router`: React Server Components, in the browser.
//
// A route's tree reaches the browser as React's Flight payload
// (ubugeeei-prod/uf#519), by one of two doors. The first page arrives inside
// its document, as the chunks `./flight-chunks.js` describes, and is read here
// while the document is still streaming so that `hydrateRoot` can start before
// the slowest boundary has resolved. Every page after that is a `fetch` of the
// route's payload URL (`./flight.js`), read the same way by React's own client.
//
// # The one hook React's Parcel client asks for
//
// `react-server-dom-parcel` resolves a client reference through a global
// `parcelRequire`: `load(url)` for each chunk a reference names, then
// `parcelRequire(id)` for the module, synchronously, once they have loaded. uf
// has no Parcel; this module is that hook. A reference's id *is* its chunk's
// URL — `@uniflowed/vite` writes it that way — so loading a chunk and
// remembering its namespace under the same string is the whole of it.
//
// It imports same-origin URLs and nothing else. The payload comes from the
// page's own origin, so a URL that points somewhere else is not a URL uf wrote,
// and turning it into an `import()` would be letting bytes decide which script
// runs.

import {
  createFromFetch,
  createFromReadableStream,
  setServerCallback,
} from "react-server-dom-parcel/client.browser";

import { createServerReference } from "../action.js";
import { withDeployment } from "./deployment.js";
import { FLIGHT_CHUNK_ATTRIBUTE, flightChunkBytes } from "./flight-chunks.js";
import {
  FLIGHT_CONTENT_TYPE,
  INTERCEPTED_FROM_HEADER,
  type FetchedFlight,
  type FlightFetchOptions,
  type FlightRoot,
  documentPathOf,
  flightUrl,
} from "./flight.js";
import { requireServerComponentsReact } from "./react-version.js";

/** A module namespace, as far as the loader looks into one. */
type ModuleNamespace = { +[string]: mixed };

/** The entry a refusal names: the one an application reaches this module through. */
const ENTRY = "@uniflowed/router/rsc/client";

/**
 * Install the module hook React's Flight client resolves references through.
 *
 * Once per page. Defined rather than assigned, for the reason
 * `./boundaries.js` gives about its own global: a name a page already defined
 * as an accessor cannot be assigned to, and hydration must not throw over it.
 */
export function installBrowserModules(): void {
  requireServerComponentsReact(ENTRY);
  // A server function a Server Component passed as a prop arrives as a Flight
  // server reference, and React's client calls this with its id and arguments.
  // The id is the action's own (`registerServerFunction` in `../rsc.js`), so
  // the call is the one an imported action makes — same grammar, same
  // endpoint, same deployment check. See ubugeeei-prod/uf#1359.
  setServerCallback(callServerFunction);
  const loaded: Map<string, ModuleNamespace> = new Map();
  // A function with three properties, which is the shape React's Parcel client
  // calls: `parcelRequire(id)` for a module, `parcelRequire.load(url)` for the
  // chunk it lives in. Declared and then given its properties, so the hook is
  // built rather than merged into something that already existed.
  function parcelRequire(id: string): ModuleNamespace {
    const namespace = loaded.get(id);
    if (namespace == null) {
      throw new Error(
        `@uniflowed/router: the client module ${id} was required before it loaded. React's ` +
          "Flight client loads every chunk a reference names before it asks for the module.",
      );
    }
    return namespace;
  }
  parcelRequire.load = (url: string): Promise<void> => {
    const target = new URL(url, window.location.href);
    if (target.origin !== window.location.origin) {
      return Promise.reject(
        new Error(
          `@uniflowed/router: a payload named the client module ${url}, which is not on this ` +
            "page's origin. uf writes same-origin chunk URLs only, so it is not loaded.",
        ),
      );
    }
    return import(target.href).then((namespace: ModuleNamespace) => {
      loaded.set(url, namespace);
    });
  };
  parcelRequire.extendImportMap = (): void => {
    throw new Error("@uniflowed/router: a payload asked for an import map, which uf never writes");
  };
  parcelRequire.meta = { publicUrl: "", devServer: null };
  Object.defineProperty(globalThis, "parcelRequire", {
    value: parcelRequire,
    writable: true,
    configurable: true,
  });
}

/** The parts of a `Document` the reader uses. */
type DocumentLike = interface {
  readonly querySelectorAll: (selector: string) => Iterable<ElementLike>,
  readonly documentElement: mixed,
};

/** The parts of an `Element` the reader uses. */
type ElementLike = interface {
  readonly textContent: string | null,
};

/** One reference per id, so a function passed twice is one function to call. */
const serverFunctions: Map<string, (...args: Array<mixed>) => Promise<mixed>> = new Map();

/**
 * Call a server reference Flight decoded, by its id.
 *
 * The error a failed call throws names the action by id rather than by module,
 * because a payload carries no module name — and should not: a name is for the
 * build's log, where the endpoint writes it.
 */
export function callServerFunction(id: string, args: Array<mixed>): Promise<mixed> {
  let call = serverFunctions.get(id);
  if (call == null) {
    call = createServerReference(id, `server function ${id.slice(0, 12)}…`) as $FlowFixMe;
    serverFunctions.set(id, call);
  }
  return call(...args);
}

/**
 * The payload a document carries, as the byte stream React's client reads.
 *
 * Every chunk element already in the document is read at once, in document
 * order, and a `MutationObserver` reads the ones the server has not written
 * yet. The stream ends at the end marker and the observer is disconnected with
 * it, so a prerendered document — every chunk already there — never installs
 * one.
 *
 * `observe` is passed in rather than reached for, so the reader can be driven
 * by a test with a document it mutates by hand; `domObserver` in
 * `./payload-rows.js` is the browser's.
 */
export function documentPayload(
  document: DocumentLike,
  observe: ?(callback: () => void) => (() => void) | null,
): ReadableStream<Uint8Array> {
  const seen: WeakSet<ElementLike> = new WeakSet();
  let stop: (() => void) | null = null;
  let done = false;
  return new ReadableStream({
    start(controller) {
      const sweep = () => {
        if (done) {
          return;
        }
        for (const element of document.querySelectorAll(`script[${FLIGHT_CHUNK_ATTRIBUTE}]`)) {
          if (seen.has(element)) {
            continue;
          }
          seen.add(element);
          let bytes;
          try {
            bytes = flightChunkBytes(element.textContent ?? "null");
          } catch (error) {
            done = true;
            stop?.();
            controller.error(error);
            return;
          }
          if (bytes == null) {
            done = true;
            stop?.();
            controller.close();
            return;
          }
          controller.enqueue(bytes);
        }
      };
      sweep();
      if (!done && observe != null) {
        stop = observe(sweep);
      }
    },
    cancel() {
      done = true;
      stop?.();
    },
  });
}

/** Read the payload a document carries into its root value. */
export function readDocumentPayload(
  document: DocumentLike,
  observe: ?(callback: () => void) => (() => void) | null,
): Promise<FlightRoot> {
  requireServerComponentsReact(ENTRY);
  return createFromReadableStream(documentPayload(document, observe));
}

/**
 * Fetch `url`'s payload.
 *
 * A redirect is followed by `fetch`, and the route the payload is for is read
 * back off the URL the answer came from — so a loader's `redirect()` during a
 * navigation lands the history entry on the target, as a document request
 * would have. Anything that is not a payload is handed back as a document to
 * load instead of being fed to React's client.
 *
 * The status is not what decides, the content type is. A route that resolved
 * to its not-found or error boundary is answered with a 404 or a 500 *and a
 * payload*, and that payload is the page to show; what is not a payload — a
 * static host's `404.html` for a file it does not have, a middleware's own
 * refusal — is a document, whatever its status.
 */
export async function fetchFlight(
  url: string,
  options?: FlightFetchOptions,
): Promise<FetchedFlight> {
  // Outside the `try` below, which answers every failure to fetch with a
  // document load: too old a React is not a network failure, and a navigation
  // that quietly reloaded the page would hide it.
  requireServerComponentsReact(ENTRY);
  // With the page's build, so a server on another one refuses the request
  // rather than answering with a payload that names chunks this page does not
  // have. The refusal is not a payload, so it becomes a document load below.
  const headers = new Headers(withDeployment({ accept: FLIGHT_CONTENT_TYPE }));
  const interceptedFrom = options?.interceptedFrom;
  if (interceptedFrom != null) {
    headers.set(INTERCEPTED_FROM_HEADER, interceptedFrom);
  }
  let response: Response;
  try {
    response = await fetch(flightUrl(url), {
      credentials: "same-origin",
      headers,
    });
  } catch {
    // A request that could not be made or followed: a dropped connection, or a
    // redirect to another origin — a sign-in page — which `fetch` may not
    // follow without CORS. The browser can still load the document, and a
    // top-level navigation follows any redirect, so that is what the router
    // does rather than leave the click doing nothing.
    return { kind: "document", url };
  }
  const landed = new URL(response.url, window.location.href);
  const document = documentPathOf(landed.pathname);
  const type = response.headers.get("content-type") ?? "";
  if (
    landed.origin !== window.location.origin ||
    document == null ||
    !type.startsWith(FLIGHT_CONTENT_TYPE)
  ) {
    void response.body?.cancel();
    // The URL the reader asked for when the redirect did not end on a payload,
    // rather than the one it ended on. A middleware that answered with its
    // sign-in page wrote a `next=` naming the payload URL; loading the document
    // URL lets it see the document and name that instead.
    return { kind: "document", url: document == null ? url : `${document}${landed.search}` };
  }
  return {
    kind: "flight",
    url: `${document}${landed.search}`,
    root: createFromFetch(Promise.resolve(response)),
  };
}
