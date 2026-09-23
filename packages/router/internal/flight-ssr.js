// @flow
//
// Internal to `@uniflowed/router`: React Server Components, while HTML renders.
//
// The HTML renderer does not render a route's modules. It reads the payload
// the Flight renderer wrote (`../rsc.js`) with React's own Flight client and
// renders what comes out, so the document is the payload's tree and not a
// second rendering of the same modules — which is what lets a Server Component
// be a module that neither this graph nor the browser ever evaluates.
//
// # Where a client component's server copy comes from
//
// A client reference in a payload names a chunk URL, which is the browser's
// module. HTML still has to be rendered from the component, on the server, so
// `@uniflowed/vite` builds a server copy of every client module and hands the
// renderer a table from the browser's URL to that copy. React's Parcel client
// resolves references through a global `parcelRequire`, exactly as in the
// browser (`./flight-browser.js`), and this module is that hook on a server.
//
// The same global serves one application per process, which is what a uf
// server is. `meta.publicUrl` is empty because the URLs a reference names are
// already absolute paths: React asks the document to preload each chunk while
// it renders, and those tags are right exactly as the payload spells them.

import { createFromReadableStream } from "react-server-dom-parcel/client.edge";

import type { FlightRoot } from "./flight.js";
import { requireServerComponentsReact } from "./react-version.js";

/** A module namespace, as far as the hook looks into one. */
type ModuleNamespace = { +[string]: mixed };

/** The server copy of the client module at a browser chunk URL. */
export type ClientModuleLoader = (url: string) => Promise<ModuleNamespace>;

/** The entry a refusal names: the one an application reaches this module through. */
const ENTRY = "@uniflowed/router/rsc/ssr";

/**
 * Install the module hook React's Flight client resolves references through.
 *
 * Idempotent over the same loader and replaced by a different one, so a dev
 * server that rebuilds its table installs the new table rather than keeping a
 * stale one.
 */
export function installServerModules(load: ClientModuleLoader): void {
  requireServerComponentsReact(ENTRY);
  const loaded: Map<string, ModuleNamespace> = new Map();
  // The browser's hook, on a server; `./flight-browser.js` has the shape.
  function parcelRequire(id: string): ModuleNamespace {
    const namespace = loaded.get(id);
    if (namespace == null) {
      throw new Error(
        `@uniflowed/router: the client module ${id} was required before its server copy loaded`,
      );
    }
    return namespace;
  }
  parcelRequire.load = (url: string): Promise<void> =>
    load(url).then((namespace) => {
      loaded.set(url, namespace);
    });
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

/**
 * Read a payload into its root value, as React's client does in the browser.
 *
 * `partial` is for a payload that is missing rows on purpose: a static shell's,
 * with the parts a partial prerender left for the request taken out
 * (`./flight-rows.js`). React's client then keeps waiting on those parts once
 * the stream has ended, rather than failing every one of them with
 * "Connection closed", so the HTML renderer leaves each as a hole.
 */
export function readPayload(
  stream: ReadableStream<Uint8Array>,
  options?: {| +partial?: boolean |},
): Promise<FlightRoot> {
  requireServerComponentsReact(ENTRY);
  return options?.partial === true
    ? createFromReadableStream(stream, { unstable_allowPartialStream: true })
    : createFromReadableStream(stream);
}
