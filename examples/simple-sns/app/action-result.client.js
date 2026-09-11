"use client";
// @flow

import { promise, runPromiseExit } from "@uniflowed/effect";

import { failed, type ActionResult } from "./social-model.js";

/** Preserve domain results and normalize rejected transports at the client boundary. */
export async function callAction<T>(
  request: () => Promise<ActionResult<T>>,
  message: string,
): Promise<ActionResult<T>> {
  const result = await runPromiseExit(promise(request));

  return match (result) {
    {kind: "success", value: const value} => value,
    {kind: "failure", ...} => failed(message),
  };
}
