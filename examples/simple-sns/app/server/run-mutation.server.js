// @flow

import { layerMerge, layerSucceed, provide, runPromiseExit, type Cause } from "@uniflowed/effect";
import { viewer } from "./session.server.js";
import { insertPost, setReaction, insertMessage, saveSettings } from "./repository.server.js";
import {
  IdentityService,
  SocialStore,
  type Mutation,
  type MutationProblem,
  type Identity,
  type Store,
} from "./programs.server.js";
import { succeeded, failed, type ActionResult } from "../social-model.js";

// Request identity is resolved when the Effect runs; there is no global current user.
const live = layerMerge(
  layerSucceed(IdentityService, { current: viewer }),
  layerSucceed(SocialStore, { insertPost, setReaction, insertMessage, saveSettings }),
);

function rejected(cause: Cause<MutationProblem>): ActionResult<empty> {
  return match (cause) {
    {kind: "fail", error: {kind: "unauthenticated"}} => failed("Please sign in to continue."),
    {kind: "fail", error: {kind: "validation", message: const message, fields: const fields}} =>
      failed(message, fields),
    {kind: "interrupt"} => failed("The request was cancelled. Please try again."),
    {kind: "empty"}
      | {kind: "die", defect: _}
      | {kind: "parallel", causes: _}
      | {kind: "sequential", causes: _} =>
      failed("Your changes could not be saved. Please try again."),
  };
}

/**
 * Provide request-time services and turn an Effect exit into a serializable action result.
 * Expected failures retain field feedback; defects are logged and receive a generic public message.
 */
export async function runMutation<T>(
  program: Mutation<T>,
  message: string,
): Promise<ActionResult<T>> {
  const result = await runPromiseExit(
    provide<T, MutationProblem, empty, Identity | Store, empty, empty>(program, live),
  );
  if (
    result.kind === "failure" &&
    result.cause.kind !== "fail" &&
    result.cause.kind !== "interrupt"
  )
    console.error("Commonplace mutation defect", result.cause);

  return match (result) {
    {kind: "success", value: const value} => succeeded(value, message),
    {kind: "failure", cause: const cause} => rejected(cause),
  };
}
