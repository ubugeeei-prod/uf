// @flow
// Public view models and the result types every screen matches on. They are the
// web Commonplace examples' model, so the two apps describe one product.

/** Closed set of channels, as the web examples store them. */

export type Topic = "design" | "release" | "runtime" | "community";

export type TopicFilter = Topic | "all";

export type User = {|
  readonly id: string,
  readonly name: string,
  readonly handle: string,
  readonly avatar: string,
  readonly bio: string,
|};

/** A published note with reaction state calculated for the current viewer. */

export type Post = {|
  readonly id: string,
  readonly author: User,
  readonly body: string,
  readonly topic: Topic,
  readonly createdAt: string,
  readonly likes: number,
  readonly liked: boolean,
|};

/** A conversation entry whose direction is relative to the reader. */

export type Message = {|
  readonly id: string,
  readonly author: "me" | "them",
  readonly body: string,
  readonly sentAt: string,
|};

export type Thread = {|
  readonly id: string,
  readonly participant: User,
  readonly lastMessage: string,
|};

/** Private profile fields, shown only to the account that owns them. */

export type Settings = {|
  readonly displayName: string,
  readonly bio: string,
  readonly email: string,
|};

export type Account = {|
  readonly name: string,
  readonly handle: string,
  readonly email: string,
|};

export type FeedFilter = {| readonly topic: TopicFilter, readonly search: string |};

/** Identity for rendering, never proof of authorization for a write. */

export type Session =
  | {| readonly kind: "guest" |}
  | {| readonly kind: "authenticated", readonly user: User |};

/** A private read either requires an account or carries that account's value. */

export type Protected<out T> =
  | {| readonly kind: "unauthenticated" |}
  | {| readonly kind: "ready", readonly value: T |};

/** Distinguish a signed-out reader, a thread that is not theirs, and a ready conversation. */

export type ConversationData =
  | {| readonly kind: "unauthenticated" |}
  | {| readonly kind: "missing" |}
  | {|
      readonly kind: "ready",
      readonly thread: Thread,
      readonly messages: $ReadOnlyArray<Message>,
    |};

/** A completed write: success carries the committed value, failure what to tell the person. */

export type ActionResult<out T> =
  | {| readonly status: "success", readonly value: T, readonly message: string |}
  | {| readonly status: "error", readonly message: string |};

/** An action's state before or after it ran; pending belongs to React's Actions. */

export type FormState<out T> = {| readonly status: "idle" |} | ActionResult<T>;

export const IDLE: FormState<empty> = { status: "idle" };

export function succeeded<T>(value: T, message: string = ""): ActionResult<T> {
  return { status: "success", value, message };
}

export function failed(message: string): ActionResult<empty> {
  return { status: "error", message };
}

export const TOPICS: $ReadOnlyArray<Topic> = ["design", "release", "runtime", "community"];

export const MAX_POST_LENGTH: number = 500;

export const MAX_MESSAGE_LENGTH: number = 2000;

/** Map each stored channel identifier to its visible label. */

export function topicLabel(topic: Topic): string {
  return match (topic) {
    "design" => "Design",
    "release" => "Shipping",
    "runtime" => "Engineering",
    "community" => "Community",
  };
}

/** The channel a new note goes to while the feed shows `filter`. */

export function defaultTopic(filter: TopicFilter): Topic {
  return match (filter) {
    "all" => "community",
    "design" => "design",
    "release" => "release",
    "runtime" => "runtime",
    "community" => "community",
  };
}

/**
 * A note's day, in the web examples' format. They print UTC so that the server's markup and the
 * browser's agree; nothing is rendered twice here, so the person sees their own time zone.
 */

export function displayDate(value: string): string {
  return new Intl.DateTimeFormat("en", { month: "short", day: "numeric" }).format(new Date(value));
}

/** A message's time, in the device's time zone. */

export function displayTime(value: string): string {
  return new Intl.DateTimeFormat("en", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).format(new Date(value));
}

/** An avatar fallback from at most two words of a display name. */

export function initials(name: string): string {
  return name
    .trim()
    .split(/\s+/)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? "")
    .join("");
}
