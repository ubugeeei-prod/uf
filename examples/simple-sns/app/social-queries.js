"use server";
// @flow

import {
  getSettings,
  getViewer,
  listMessages,
  listPosts,
  listThreads,
} from "./social-db.server.js";
import {
  statsFor,
  type FeedStats,
  type Message,
  type MessageThread,
  type Post,
  type Settings,
  type User,
} from "./social-model.js";

type TimelineData = {|
  readonly posts: Array<Post>,
  readonly stats: FeedStats,
  readonly viewer: User,
|};

type MessagesData = {|
  readonly messages: Array<Message>,
  readonly threads: Array<MessageThread>,
|};

export async function timelineData(): Promise<TimelineData> {
  const [viewer, posts] = await Promise.all([getViewer(), listPosts()]);
  return { viewer, posts, stats: statsFor(posts) };
}

export async function messagesData(): Promise<MessagesData> {
  const threads = await listThreads();
  const groups = await Promise.all(threads.map((thread) => listMessages(thread.id)));
  return { threads, messages: groups.flat() };
}

export async function settingsData(): Promise<Settings> {
  return getSettings();
}
