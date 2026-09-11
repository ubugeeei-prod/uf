// @flow

import { catchAll, die, fail, type Effect } from "@uniflowed/effect";

import { InputError } from "./validation.server.js";
import type { FieldErrors } from "../social-model.js";

/** Expected input or ownership rejection, safe to return to the submitting user. */
export type InputProblem = {|
  readonly kind: "validation",
  readonly message: string,
  readonly fields: FieldErrors,
|};

/**
 * Adapt the synchronous repository and asynchronous credential APIs to Effect.
 * Only InputError enters the typed failure channel; database faults and bugs
 * remain defects and must not be presented as invalid user input.
 */
export function inputEffect<T>(operation: Effect<T, mixed>): Effect<T, InputProblem> {
  return catchAll(operation, (error) =>
    error instanceof InputError
      ? fail({ kind: "validation", message: error.message, fields: error.fields })
      : die(error),
  );
}
