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
import { encodeActionArguments } from "./action-wire.js";
import { type FormActionFields, formFields } from "./form-action.js";
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
    ? createFromReadableStream(stream, { unstable_allowPartialStream: true, encodeFormAction })
    : createFromReadableStream(stream, { encodeFormAction });
}

/** A bound-arguments promise, as far as this module has seen it settle. */
type Settled = {| status: "pending" | "fulfilled" | "rejected", value: mixed |};
const settled: WeakMap<Promise<mixed>, Settled> = new WeakMap();

/**
 * The form fields for a server reference a payload carried as a prop.
 *
 * React's Flight client calls this from the reference's `$$FORM_ACTION` while
 * the HTML renders, so a `<form action={fn}>` in a Client Component posts
 * before hydration whether `fn` was imported or handed down by a Server
 * Component (ubugeeei-prod/uf#1358, #1359). The fields are
 * `./form-action.js`'s, which the endpoint's second door reads.
 *
 * Two differences from an imported action's. The bound arguments arrive as a
 * promise, so a render that meets one before it has settled suspends on it —
 * the promise is thrown, which React's HTML renderer waits on and retries. That
 * needs the promise to be the same one on the retry, which is why
 * `registerServerFunction` in `../rsc.js` gives every reference a bound list,
 * empty or not: the payload's row for it is one promise for the whole render,
 * where React's stand-in for "nothing bound" is a new one per call. And
 * React does not pass the form's prefix here, so the prefix is derived from the
 * id and the bound arguments: two buttons in one form bound to different
 * arguments get different fields, and two bound the same are the same submit.
 */
export function encodeFormAction(id: string, bound: Promise<Array<mixed>>): FormActionFields {
  let state = settled.get(bound);
  if (state == null) {
    const tracked: Settled = { status: "pending", value: undefined };
    state = tracked;
    settled.set(bound, tracked);
    bound.then(
      (value) => {
        tracked.status = "fulfilled";
        tracked.value = value;
      },
      (reason) => {
        tracked.status = "rejected";
        tracked.value = reason;
      },
    );
  }
  if (state.status === "pending") {
    throw bound;
  }
  if (state.status === "rejected") {
    throw state.value;
  }
  const args: Array<mixed> = Array.isArray(state.value) ? state.value : [];
  return formFields(id, args, referencePrefix(id, args));
}

/**
 * A short prefix that is the same for the same id and bound arguments.
 *
 * FNV-1a over the id and the arguments as the wire writes them, in base 36.
 * Not a secret and not a checksum: it only keeps two forms' fields apart.
 */
function referencePrefix(id: string, args: $ReadOnlyArray<mixed>): string {
  const text = args.length === 0 ? id : `${id}:${encodeActionArguments(args)}`;
  let hash = 0x811c9dc5;
  for (let index = 0; index < text.length; index += 1) {
    hash ^= text.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return `s${hash.toString(36)}`;
}
