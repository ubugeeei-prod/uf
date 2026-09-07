// @flow
//
// What an action reaches for, and the browser must not.
//
// An ordinary server module with no directive of its own: it is server code
// because the only thing that imports it is a `"use server"` module, and a
// `"use server"` module is not something the browser evaluates. Standing in
// here for the database handle, the API key and the privileged helper that
// this position holds in a real application — none of which has any business
// in a bundle a visitor downloads.

/** The string that proves this module reached a bundle it should not have. */
export const LEDGER_MARKER: string = "ledger-marker-the-browser-must-never-see";

/** What the tally is worth, which is the ledger's to decide and not the page's. */
export function tallyFor(count: number): number {
  return count * 2 + 1;
}
