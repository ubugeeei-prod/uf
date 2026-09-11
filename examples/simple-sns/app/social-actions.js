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

export async function createPost(
  _previous: FormState<Post>,
  form: FormData,
): Promise<ActionResult<Post>> {
  return runMutation(publishNote(form), "Note published.");
}
export async function likePost(id: string, liked: boolean): Promise<ActionResult<Post>> {
  return runMutation(
    appreciateNote(id, liked),
    liked ? "Appreciation added." : "Appreciation removed.",
  );
}
export async function sendMessage(
  _previous: FormState<Message>,
  form: FormData,
): Promise<ActionResult<Message>> {
  return runMutation(deliverMessage(form), "Message sent.");
}
export async function updateSettings(
  _previous: FormState<Settings>,
  form: FormData,
): Promise<ActionResult<Settings>> {
  return runMutation(changeProfile(form), "Your changes are saved.");
}
