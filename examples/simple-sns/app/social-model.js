// @flow
// Public, serializable view models. Database rows and credentials stay on the server.
export type View = "timeline" | "clips" | "messages" | "settings" | "login" | "signup";
export type Topic = "release" | "runtime" | "design" | "community";
export type User = {|
  readonly id: string,
  readonly name: string,
  readonly handle: string,
  readonly avatar: string,
  readonly photo?: string | null,
  readonly bio: string,
|};
export type Post = {|
  readonly id: string,
  readonly author: User,
  readonly body: string,
  readonly topic: Topic,
  readonly likes: number,
  readonly liked: boolean,
  readonly createdAt: string,
|};
export type Settings = {|
  readonly displayName: string,
  readonly handle: string,
  readonly bio: string,
  readonly email: string,
|};
export type MessageThread = {|
  readonly id: string,
  readonly name: string,
  readonly handle: string,
  readonly avatar: string,
  readonly photo?: string | null,
  readonly lastMessage: string,
|};
export type Message = {|
  readonly id: string,
  readonly threadId: string,
  readonly author: "me" | "them",
  readonly body: string,
  readonly sentAt: string,
|};
// Success always carries a value; failure never can. Pending belongs to React's Action.
export type FieldErrors = { readonly [string]: string };
export type ActionResult<out T> =
  | {| readonly status: "success", readonly value: T, readonly message: string |}
  | {| readonly status: "error", readonly message: string, readonly fields: FieldErrors |};
export type FormState<out T> = {| readonly status: "idle" |} | ActionResult<T>;
export type Session =
  | {| readonly kind: "guest" |}
  | {| readonly kind: "authenticated", readonly user: User |};
export type Protected<out T> =
  | {| readonly kind: "unauthenticated" |}
  | {| readonly kind: "ready", readonly value: T |};
export type FeedFilter = {|
  readonly topic: Topic | "all",
  readonly query: string,
  readonly page: number,
|};
export type FeedData = {|
  ...FeedFilter,
  readonly posts: $ReadOnlyArray<Post>,
  readonly hasNext: boolean,
|};
export type InboxData = Protected<$ReadOnlyArray<MessageThread>>;
export type ConversationData =
  | {| readonly kind: "unauthenticated" |}
  | {| readonly kind: "empty" |}
  | {| readonly kind: "missing" |}
  | {|
      readonly kind: "ready",
      readonly thread: MessageThread,
      readonly messages: $ReadOnlyArray<Message>,
    |};
export const GUEST: Session = { kind: "guest" };
export const IDLE: FormState<empty> = { status: "idle" };
export function succeeded<T>(value: T, message: string): ActionResult<T> {
  return { status: "success", value, message };
}
export function failed(message: string, fields: FieldErrors = {}): ActionResult<empty> {
  return { status: "error", message, fields };
}
export function fieldError(state: FormState<mixed>, name: string): string | null {
  return match (state) {
    {status: "idle"} | {status: "success", ...} => null,
    {status: "error", fields: const fields, ...} => fields[name] ?? null,
  };
}
export function feedFilter(topic: string, query: string, page: string): FeedFilter {
  const parsed = Number(page);
  return {
    topic: topicFrom(typeof topic === "string" ? topic : "") ?? "all",
    query: typeof query === "string" ? query.trim().slice(0, 100) : "",
    page: Number.isInteger(parsed) ? Math.min(1000, Math.max(1, parsed)) : 1,
  };
}
export const MAX_POST_LENGTH: number = 500;
export const MAX_MESSAGE_LENGTH: number = 2000;
export const PAGE_SIZE: number = 12;
export const TOPICS: $ReadOnlyArray<Topic> = ["design", "release", "runtime", "community"];
export function topicLabel(topic: Topic): string {
  return match (topic) {
    "design" => "Design",
    "release" => "Shipping",
    "runtime" => "Engineering",
    "community" => "Community",
  };
}
export function topicFrom(value: string): Topic | null {
  return match (value) {
    "design" => "design",
    "release" => "release",
    "runtime" => "runtime",
    "community" => "community",
    _ => null,
  };
}
export function profileInitials(user: User | Settings): string {
  const name = "displayName" in user ? user.displayName : user.name;
  return name
    .trim()
    .split(/\s+/)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? "")
    .join("");
}
export function feedHref(topic: Topic | "all", query: string = "", page: number = 1): string {
  const search = new URLSearchParams();
  if (topic !== "all") search.set("topic", topic);
  if (query !== "") search.set("q", query);
  if (page > 1) search.set("page", String(page));
  return search.size === 0 ? "/" : `/?${search.toString()}`;
}
export function displayDate(value: string): string {
  // Explicit UTC keeps server and browser markup identical.
  return new Intl.DateTimeFormat("en", { month: "short", day: "numeric", timeZone: "UTC" }).format(
    new Date(value),
  );
}
export function displayTime(value: string): string {
  return new Intl.DateTimeFormat("en", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
    timeZone: "UTC",
  }).format(new Date(value));
}

// The fixture identities use licensed portraits; newly created accounts keep initials.
export function avatarPhoto(id: string): string | null {
  return match (id) {
    "seed-mika" => "/media/avatars/mika.jpg",
    "seed-ren" => "/media/avatars/ren.jpg",
    "seed-sora" => "/media/avatars/sora.jpg",
    "seed-niko" => "/media/avatars/niko.jpg",
    _ => null,
  };
}
