"use server";
// @flow
//
// The server action the counter calls.
//
// A `"use server"` module: every export is a function the browser may invoke
// by id over the network, and nothing in this file is code the browser has.
// `@uniflowed/vite` gives the client bundle one `createServerReference` per
// export of this module, so what the button holds is an id and a `fetch`.
//
// Both of the things this file reaches for are the point of it. `cookies()`
// comes from `@uniflowed/server`, which imports `node:async_hooks` — the
// import ubugeeei-prod/uf#252 names as the one a page that calls it used to
// put in the browser's graph with nothing to stop it. `./ledger.js` is the
// module an action reaches that has no directive of its own and is server code
// anyway, because the only thing that imports it is this.
//
// `crates/uf_cli/tests/vite.rs` builds this project and asserts that neither
// marker below is in `dist/assets/*.js`, that `node:async_hooks` is not
// either, and that the reference that replaced this module is.

import { cookies } from "@uniflowed/server";

import { LEDGER_MARKER, tallyFor } from "./ledger.js";

/**
 * The string that proves this module reached a bundle it should not have.
 *
 * Not exported: a `"use server"` module may only export async functions, and
 * the RSC graph reports anything else. A string literal in a function body
 * survives minification exactly as an exported one would, which is all the
 * assertion needs.
 */
const TALLY_MARKER = "tally-marker-only-the-server-runs-this";

/** Record a count and answer with what the server made of it. */
export async function recordCount(count: number): Promise<{|
  readonly total: number,
  readonly marker: string,
  readonly visitor: string,
|}> {
  return {
    total: tallyFor(count),
    marker: `${TALLY_MARKER}/${LEDGER_MARKER}`,
    // Proof that an action runs inside the request the host began: outside one
    // this throws rather than answering `null`.
    visitor: cookies().get("visitor") ?? "anonymous",
  };
}
