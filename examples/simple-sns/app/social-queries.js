"use server";
// @flow
// These are callable read capabilities. uf does not yet send a Flight component tree.
// Each read establishes its own identity, even when invoked outside a page loader.
import { viewer } from "./server/session.server.js";
import { listPosts, listThreads, listMessages, settingsFor } from "./server/repository.server.js";
import {
  PAGE_SIZE,
  feedFilter,
  GUEST,
  type Session,
  type FeedData,
  type InboxData,
  type ConversationData,
  type Protected,
  type Settings,
} from "./social-model.js";

export async function sessionData(): Promise<Session> {
  const user = viewer();
  return user == null ? GUEST : { kind: "authenticated", user };
}
export async function timelineData(
  topic: string = "all",
  query: string = "",
  page: string = "1",
): Promise<FeedData> {
  const current = viewer();
  const filter = feedFilter(topic, query, page);
  const rows = listPosts(current?.id ?? null, filter.topic, filter.query, filter.page);
  return { ...filter, posts: rows.slice(0, PAGE_SIZE), hasNext: rows.length > PAGE_SIZE };
}
export async function threadsData(): Promise<InboxData> {
  const current = viewer();
  return current == null
    ? { kind: "unauthenticated" }
    : { kind: "ready", value: listThreads(current) };
}
export async function messagesData(threadId: string = ""): Promise<ConversationData> {
  const current = viewer();
  if (current == null) return { kind: "unauthenticated" };
  const threads = listThreads(current);
  const selected = threadId === "" ? threads[0] : threads.find((thread) => thread.id === threadId);
  if (selected == null) return threadId === "" ? { kind: "empty" } : { kind: "missing" };
  // Only one conversation crosses the boundary, after membership was checked.
  return { kind: "ready", thread: selected, messages: listMessages(current, selected.id) };
}
export async function settingsData(): Promise<Protected<Settings>> {
  const current = viewer();
  return current == null
    ? { kind: "unauthenticated" }
    : { kind: "ready", value: settingsFor(current) };
}
