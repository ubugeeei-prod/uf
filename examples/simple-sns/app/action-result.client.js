"use client";
// @flow
import { failed, type ActionResult } from "./social-model.js";
// Normalize a failed transport once. Domain errors already arrive as ActionResult.
export async function callAction<T>(
  request: () => Promise<ActionResult<T>>,
  message: string,
): Promise<ActionResult<T>> {
  try {
    return await request();
  } catch {
    return failed(message);
  }
}
