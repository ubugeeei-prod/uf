"use server";
// @flow
// Thin transport adapters. Untrusted previous state never selects an identity or a record.
import {
  publishNote,
  appreciateNote,
  deliverMessage,
  changeProfile,
} from "./server/programs.server.js";
import { runMutation } from "./server/run-mutation.server.js";
import type { ActionResult, FormState, Post, Message, Settings } from "./social-model.js";

/** Publish through the authenticated Effect program; previous form state is never trusted. */
export async function createPost(
  _previous: FormState<Post>,
  form: FormData,
): Promise<ActionResult<Post>> {
  return runMutation(publishNote(form), "Note published.");
}

/** Apply an intended reaction state, so retries cannot accidentally toggle it twice. */
export async function likePost(id: string, liked: boolean): Promise<ActionResult<Post>> {
  return runMutation(
    appreciateNote(id, liked),
    liked ? "Appreciation added." : "Appreciation removed.",
  );
}

/** Deliver through the authenticated Effect program with repository membership checks. */
export async function sendMessage(
  _previous: FormState<Message>,
  form: FormData,
): Promise<ActionResult<Message>> {
  return runMutation(deliverMessage(form), "Message sent.");
}

/** Update the current account only; submitted previous state cannot select another account. */
export async function updateSettings(
  _previous: FormState<Settings>,
  form: FormData,
): Promise<ActionResult<Settings>> {
  return runMutation(changeProfile(form), "Your changes are saved.");
}
