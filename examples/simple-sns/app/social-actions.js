"use server";
// @flow

import {
  clampMessageBody,
  clampPostBody,
  normalizeHandle,
  type FormState,
  type Message,
  type Post,
  type Settings,
  type Topic,
  type User,
} from "./social-model.js";
import {
  insertMessage,
  insertPost,
  likePostById,
  saveSettings,
  upsertDemoUser,
} from "./social-db.server.js";

function text(form: FormData, name: string): string {
  const value = form.get(name);
  return typeof value === "string" ? value : "";
}

function bool(form: FormData, name: string): boolean {
  return form.get(name) === "on";
}

function topic(form: FormData): Topic {
  const value = text(form, "topic");
  return match (value) {
    "release" => "release",
    "runtime" => "runtime",
    "design" => "design",
    "community" => "community",
    _ => "community",
  };
}

export async function createPost(
  previous: FormState<Post>,
  form: FormData,
): Promise<FormState<Post>> {
  const body = clampPostBody(text(form, "body"));
  if (body.length === 0) {
    return { status: "error", message: "Write something before posting.", value: previous.value };
  }
  const post = await insertPost(body, topic(form));
  return { status: "success", message: "Posted to the timeline.", value: post };
}

export async function likePost(
  id: string,
): Promise<{| readonly id: string, readonly likes: number |}> {
  return { id, likes: await likePostById(id) };
}

export async function sendMessage(
  previous: FormState<Message>,
  form: FormData,
): Promise<FormState<Message>> {
  const threadId = text(form, "threadId");
  const body = clampMessageBody(text(form, "body"));
  if (threadId.length === 0 || body.length === 0) {
    return {
      status: "error",
      message: "Choose a thread and write a message.",
      value: previous.value,
    };
  }
  const message = await insertMessage(threadId, body);
  return { status: "success", message: "Message sent.", value: message };
}

export async function signIn(previous: FormState<User>, form: FormData): Promise<FormState<User>> {
  const rawHandle = text(form, "handle").trim();
  if (rawHandle.length === 0) {
    return { status: "error", message: "Enter a handle.", value: previous.value };
  }
  const handle = normalizeHandle(rawHandle);
  const user = await upsertDemoUser(handle, handle, "Signed in with the demo account.");
  return { status: "success", message: `Welcome back, @${user.handle}.`, value: user };
}

export async function signUp(previous: FormState<User>, form: FormData): Promise<FormState<User>> {
  const name = text(form, "name").trim();
  const handle = normalizeHandle(text(form, "handle"));
  if (name.length === 0 || handle.length === 0) {
    return { status: "error", message: "Name and handle are required.", value: previous.value };
  }
  const user = await upsertDemoUser(name, handle, text(form, "bio"));
  return { status: "success", message: `Created @${user.handle}.`, value: user };
}

export async function updateSettings(
  previous: FormState<Settings>,
  form: FormData,
): Promise<FormState<Settings>> {
  const displayName = text(form, "displayName").trim();
  const email = text(form, "email").trim();
  if (displayName.length === 0 || email.length === 0) {
    return {
      status: "error",
      message: "Display name and email are required.",
      value: previous.value,
    };
  }
  const value = await saveSettings({
    displayName,
    handle: normalizeHandle(text(form, "handle")),
    bio: text(form, "bio").trim().slice(0, 160),
    email,
    digest: bool(form, "digest"),
    quietMode: bool(form, "quietMode"),
  });
  return { status: "success", message: "Settings saved.", value };
}
