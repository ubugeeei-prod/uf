// @flow
//
// The Node, Fetch and Intl surfaces `libdefs/node-*.js`, `fetch.js` and
// `intl-relative-time.js` declare, used the way a uf project uses them.

import { AsyncLocalStorage } from "node:async_hooks";
import fs from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";

// `pathToFileURL` answers a WHATWG `URL`, and `fileURLToPath` takes one back.
export const href: string = pathToFileURL("/tmp/entry.js").href;
export const path: string = fileURLToPath(pathToFileURL("/tmp/entry.js"));
export const fromURL: string = fileURLToPath(new URL("file:///tmp/entry.js"));

// A `Dirent`'s name is a string unless the call asked for a buffer.
export const names: Array<string> = fs
  .readdirSync("/tmp", { withFileTypes: true })
  .map((entry) => entry.name);

// A `Request` is a valid init for another `Request`.
export function moved(url: string, request: Request): Request {
  return new Request(url, request);
}

// A streaming body says it is half-duplex.
export function streamed(url: string, body: ReadableStream<Uint8Array>): Request {
  return new Request(url, { body, duplex: "half", method: "POST" });
}

// Every `Set-Cookie` header on its own.
export const cookies: Array<string> = new Headers().getSetCookie();

// A readonly table of headers or parameters is enough, since nothing writes to it.
const DEFAULTS: { readonly [string]: string } = { accept: "text/html" };
export const headers: Headers = new Headers(DEFAULTS);
export const query: URLSearchParams = new URLSearchParams(DEFAULTS);

// Relative time, and the unit type it formats.
const unit: Intl$RelativeTimeFormatUnit = "day";
export const yesterday: string = new Intl.RelativeTimeFormat("en", { numeric: "auto" }).format(
  -1,
  unit,
);
export const options: Intl.RelativeTimeFormatOptions = { numeric: "auto" };

// A store that follows the call chain.
const storage: AsyncLocalStorage<{ readonly id: string }> = new AsyncLocalStorage();
export const id: ?string = storage.run({ id: "a" }, () => storage.getStore()?.id);
