// @flow

export type View = "timeline" | "messages" | "settings" | "login" | "signup";
export type Topic = "release" | "runtime" | "design" | "community";
export type Delivery = "sent" | "delivered" | "read";

export type User = {|
  readonly id: string,
  readonly name: string,
  readonly handle: string,
  readonly avatar: string,
  readonly bio: string,
|};

export type Post = {|
  readonly id: string,
  readonly author: User,
  readonly body: string,
  readonly topic: Topic,
  readonly likes: number,
  readonly replies: number,
  readonly createdAt: string,
  readonly liked: boolean,
|};

export type FeedStats = {|
  readonly posts: number,
  readonly authors: number,
  readonly likes: number,
  readonly replies: number,
|};

export type Settings = {|
  readonly displayName: string,
  readonly handle: string,
  readonly bio: string,
  readonly email: string,
  readonly digest: boolean,
  readonly quietMode: boolean,
|};

export type MessageThread = {|
  readonly id: string,
  readonly name: string,
  readonly handle: string,
  readonly unread: number,
  readonly lastMessage: string,
|};

export type Message = {|
  readonly id: string,
  readonly threadId: string,
  readonly author: "me" | "them",
  readonly body: string,
  readonly sentAt: string,
  readonly delivery: Delivery,
|};

export type FormState<T = empty> = {|
  readonly status: "idle" | "success" | "error",
  readonly message: string,
  readonly value?: T,
|};

export const MAX_POST_LENGTH: number = 280;
export const MAX_MESSAGE_LENGTH: number = 360;

export const TOPICS: $ReadOnlyArray<Topic> = ["release", "runtime", "design", "community"];

export function viewHref(view: View): string {
  return match (view) {
    "timeline" => "/",
    "messages" => "/messages",
    "settings" => "/settings",
    "login" => "/login",
    "signup" => "/signup",
  };
}

export function viewLabel(view: View): string {
  return match (view) {
    "timeline" => "Timeline",
    "messages" => "Messages",
    "settings" => "Settings",
    "login" => "Log in",
    "signup" => "Sign up",
  };
}

export function pageTitle(view: View): string {
  return match (view) {
    "timeline" => "Team timeline",
    "messages" => "Direct messages",
    "settings" => "Settings",
    "login" => "Log in",
    "signup" => "Create account",
  };
}

export function topicLabel(topic: Topic): string {
  return match (topic) {
    "release" => "Release",
    "runtime" => "Runtime",
    "design" => "Design",
    "community" => "Community",
  };
}

export function topicAccent(topic: Topic): string {
  return match (topic) {
    "release" => "#2563eb",
    "runtime" => "#047857",
    "design" => "#be123c",
    "community" => "#7c3aed",
  };
}

export function deliveryLabel(delivery: Delivery): string {
  return match (delivery) {
    "sent" => "Sent",
    "delivered" => "Delivered",
    "read" => "Read",
  };
}

export function visiblePosts(
  posts: $ReadOnlyArray<Post>,
  topic: Topic | "all",
  query: string,
): Array<Post> {
  const needle = query.trim().toLowerCase();
  return posts.filter((post) => {
    const matchesTopic = topic === "all" || post.topic === topic;
    const matchesQuery =
      needle.length === 0 ||
      post.body.toLowerCase().includes(needle) ||
      post.author.name.toLowerCase().includes(needle) ||
      post.author.handle.toLowerCase().includes(needle);
    return matchesTopic && matchesQuery;
  });
}

export function statsFor(posts: $ReadOnlyArray<Post>): FeedStats {
  const authors = new Set<string>();
  let likes = 0;
  let replies = 0;
  for (const post of posts) {
    authors.add(post.author.handle);
    likes += post.likes;
    replies += post.replies;
  }
  return { posts: posts.length, authors: authors.size, likes, replies };
}

export function normalizeHandle(input: string): string {
  const handle = input
    .trim()
    .toLowerCase()
    .replace(/^@+/, "")
    .replace(/[^a-z0-9_]/g, "");
  return handle.length === 0 ? "reader" : handle.slice(0, 20);
}

export function clampPostBody(input: string): string {
  return input.trim().slice(0, MAX_POST_LENGTH);
}

export function clampMessageBody(input: string): string {
  return input.trim().slice(0, MAX_MESSAGE_LENGTH);
}

export function profileInitials(user: User | Settings): string {
  const name = "displayName" in user ? user.displayName : user.name;
  return name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? "")
    .join("");
}

export function messagePreview(thread: MessageThread): string {
  const prefix = thread.unread > 0 ? `${String(thread.unread)} new` : "Caught up";
  return `${prefix} - ${thread.lastMessage}`;
}

export function optimisticPost(body: string, topic: Topic, viewer: User, now: Date): Post {
  return {
    id: `optimistic-${String(now.getTime())}`,
    author: viewer,
    body: clampPostBody(body),
    topic,
    likes: 1,
    replies: 0,
    createdAt: now.toISOString(),
    liked: true,
  };
}

export function optimisticMessage(threadId: string, body: string, now: Date): Message {
  return {
    id: `optimistic-${String(now.getTime())}`,
    threadId,
    author: "me",
    body: clampMessageBody(body),
    sentAt: now.toISOString(),
    delivery: "sent",
  };
}
