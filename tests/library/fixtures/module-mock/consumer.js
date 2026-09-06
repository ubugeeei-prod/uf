// @flow
//
// A module that calls its dependency while it is being evaluated.
//
// This is the case a spy cannot reach: by the time a test could call
// `uft.spyOn` on anything, `greeting` already holds what the real `send`
// returned. Only replacing the module before this one is imported changes it.

import { BASE, send } from "./client.js";

/** Computed at module scope, which is what makes this hard to test. */
export const greeting: string = send("/hello");

/** Read at module scope too, so a mocked constant is observable. */
export const base: string = BASE;
