// @flow
//
// Internal to `@uniflowed/router`: the two ids the server writes and the
// client reads.
//
// Their own module because both halves need them and neither should import the
// other. The client used to take them from `./server.js`, which meant a client
// bundle reached the request dispatcher — and through it `node:async_hooks`,
// by way of `@uniflowed/server`. Nothing was *called*, so a bundler dropped the
// code, but the import of a Node builtin survived into a browser bundle and
// Vite said so on every build.
//
// Two string constants are not worth a boundary violation.

/**
 * The element the client hydrates when the app does not render `<html>`.
 *
 * An app whose root layout renders the whole document owns it and hydrates
 * `document` instead; this is the wrapper for the ones that render content.
 */
export const ROOT_ID = "uf-root";

/** The script element the server's resolved route data is written into. */
export const DATA_ID = "__uf_data";
