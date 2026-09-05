// @flow
//
// `@uniflowed/web/cookie`: reading a cookie from a component.
//
// A cookie is one value with two readers. On a server it is on the request,
// and `@uniflowed/server` has it; in a browser it is on `document`, and a
// component can read it directly. `useCookie` is the same call in both places,
// which is the only reason it is worth having.
//
// This module deliberately does *not* import `@uniflowed/server`: that package
// imports `node:async_hooks`, and importing it here would drag a Node builtin
// into every browser bundle that renders a page. The server value arrives
// through the render instead — the router puts it there — and the browser reads
// `document.cookie`. Two sources, one call, and no server module in the client
// graph.
//
// # What belongs in this module
//
// The browser half of a cookie, and the name under which a server render hands
// its half over. A cookie is a small enough subject to be one file and a
// distinct enough one to be its own: it is the only value in this package that
// is *written* as well as read, and the only one whose two readers are
// different APIs rather than the same API in two environments.
//
// Not here: anything that reads a request. Headers, the URL, and the session
// are `@uniflowed/server`'s, and the paragraph above is the reason this module
// must not grow an import of it.

import * as React from "@uniflowed/react";

/** How a cookie is written, in the browser. */
export type CookieOptions = {
  /** Seconds until it expires. Absent means a session cookie. */
  readonly maxAge?: number,
  readonly path?: string,
  readonly sameSite?: "lax" | "strict" | "none",
  readonly secure?: boolean,
};

/** Where a server-rendered page's cookies are handed to the client half. */
const SERVER_COOKIES: string = "__ufServerCookies";

/**
 * Parse a `Cookie`-shaped header into a plain object.
 *
 * `Object.create(null)`, because `__proto__` is a cookie name an attacker can
 * set and on an ordinary object it would not be a key at all. The same
 * reasoning as `@uniflowed/server`'s parser, and deliberately a second copy:
 * sharing it would mean importing that package here.
 */
export function parse(header: string | null): { [string]: string } {
  const out: { [string]: string } = Object.create(null);
  if (header == null || header === "") {
    return out;
  }
  for (const pair of header.split(";")) {
    const at = pair.indexOf("=");
    if (at < 0) {
      continue;
    }
    const name = pair.slice(0, at).trim();
    if (name === "" || Object.hasOwn(out, name)) {
      continue;
    }
    let value = pair.slice(at + 1).trim();
    if (value.length >= 2 && value.startsWith('"') && value.endsWith('"')) {
      value = value.slice(1, -1);
    }
    try {
      out[name] = decodeURIComponent(value);
    } catch {
      // A malformed cookie is not a reason to fail a render; the value is
      // simply not what the sender meant.
      out[name] = value;
    }
  }
  return out;
}

/** Serialise one cookie for `document.cookie`. */
export function serialize(name: string, value: string, options?: CookieOptions): string {
  let out = `${encodeURIComponent(name)}=${encodeURIComponent(value)}`;
  out += `; Path=${options?.path ?? "/"}`;
  if (options?.maxAge != null) {
    out += `; Max-Age=${Math.trunc(options.maxAge)}`;
  }
  out += `; SameSite=${options?.sameSite ?? "lax"}`;
  if (options?.secure === true) {
    out += "; Secure";
  }
  return out;
}

/** Whatever cookies are readable here, server or browser. */
function readAll(): { [string]: string } {
  const host = (globalThis: $FlowFixMe);
  if (host.document != null) {
    return parse(host.document.cookie);
  }
  // Server-rendered: the router leaves the request's cookies where both halves
  // can see them, so the first render matches what the browser will read.
  return parse(host[SERVER_COOKIES] ?? null);
}

/**
 * Read a cookie, and write it in the browser.
 *
 * The value is `null` when the cookie is not set, rather than `undefined`, so a
 * component can tell "no cookie" from "a cookie whose value is empty" without
 * two comparisons.
 *
 * Writing is browser-only and says so: setting a cookie during a server render
 * has no defined moment to take effect — the headers may already be on the wire
 * — so a route handler or a server action is where that belongs.
 */
export function useCookie(
  name: string,
): [string | null, (next: string, options?: CookieOptions) => void] {
  const [value, setValue] = React.useState<string | null>(() => {
    const all = readAll();
    return Object.hasOwn(all, name) ? all[name] : null;
  });

  const write = React.useCallback(
    (next: string, options?: CookieOptions) => {
      const host = (globalThis: $FlowFixMe);
      if (host.document == null) {
        throw new Error(
          `useCookie: ${name} cannot be written during a server render. ` +
            "The response's headers may already have gone; set it from a route " +
            "handler or a server action instead.",
        );
      }
      host.document.cookie = serialize(name, next, options);
      setValue(next);
    },
    [name],
  );

  return [value, write];
}

/** Where the server leaves the request's cookies for the first render. */
export const SERVER_COOKIE_GLOBAL: string = SERVER_COOKIES;
