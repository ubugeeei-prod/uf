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

import { createFromFetch, createFromReadableStream } from "react-server-dom-parcel/client.browser";

import { FLIGHT_CHUNK_ATTRIBUTE, flightChunkBytes } from "./flight-chunks.js";
import { FLIGHT_CONTENT_TYPE, type FlightRoot, documentPathOf, flightUrl } from "./flight.js";

/** A module namespace, as far as the loader looks into one. */
type ModuleNamespace = { +[string]: mixed };

/**
 * Install the module hook React's Flight client resolves references through.
 *
 * Once per page. Defined rather than assigned, for the reason
 * `./boundaries.js` gives about its own global: a name a page already defined
 * as an accessor cannot be assigned to, and hydration must not throw over it.
 */
export function installBrowserModules(): void {
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
  return createFromReadableStream(documentPayload(document, observe));
}

/** What fetching a route's payload turned into. */
export type FetchedFlight =
  | {|
      readonly kind: "flight",
      /** The route the server answered for: a redirect's target, when there was one. */
      readonly url: string,
      readonly root: Promise<FlightRoot>,
    |}
  | {|
      /**
       * The answer was not a payload: a redirect off this origin, or a host
       * that had no payload for the URL. The browser should load `url` as a
       * document.
       */
      readonly kind: "document",
      readonly url: string,
    |};

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
export async function fetchFlight(url: string): Promise<FetchedFlight> {
  const response = await fetch(flightUrl(url), {
    credentials: "same-origin",
    headers: { accept: FLIGHT_CONTENT_TYPE },
  });
  const landed = new URL(response.url, window.location.href);
  const document = documentPathOf(landed.pathname);
  const type = response.headers.get("content-type") ?? "";
  if (
    landed.origin !== window.location.origin ||
    document == null ||
    !type.startsWith(FLIGHT_CONTENT_TYPE)
  ) {
    void response.body?.cancel();
    return { kind: "document", url: document == null ? url : `${document}${landed.search}` };
  }
  return {
    kind: "flight",
    url: `${document}${landed.search}`,
    root: createFromFetch(Promise.resolve(response)),
  };
}
