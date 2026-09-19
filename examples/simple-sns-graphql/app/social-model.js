// @flow
// Public, serializable view models. Database rows and credentials stay on the server.

/** Destinations represented by the shared navigation shell. */
export type View = "timeline" | "clips" | "messages" | "settings" | "login" | "signup";

/** Closed set of channels accepted by the UI and persistence layer. */
export type Topic = "release" | "runtime" | "design" | "community";

/** Public profile DTO. Credentials and contact email never belong in this shape. */
export type User = {|
  readonly id: string,
  readonly name: string,
  readonly handle: string,
  readonly avatar: string,
  readonly photo?: string | null,
  readonly bio: string,
|};

/** A published note with reaction state calculated for the requesting viewer. */
export type Post = {|
  readonly id: string,
  readonly author: User,
  readonly body: string,
  readonly topic: Topic,
  readonly likes: number,
  readonly liked: boolean,
  readonly createdAt: string,
|};

/** Private profile fields returned only to the account owner. */
export type Settings = {|
  readonly displayName: string,
  readonly handle: string,
  readonly bio: string,
  readonly email: string,
|};

/** An authorized inbox preview; the full conversation is loaded separately. */
export type MessageThread = {|
  readonly id: string,
  readonly name: string,
  readonly handle: string,
  readonly avatar: string,
  readonly photo?: string | null,
  readonly lastMessage: string,
|};

/** A conversation entry whose direction is relative to the authenticated reader. */
export type Message = {|
  readonly id: string,
  readonly threadId: string,
  readonly author: "me" | "them",
  readonly body: string,
  readonly sentAt: string,
|};

/** Safe validation messages keyed by the associated HTML form field name. */
export type FieldErrors = { readonly [string]: string };

/** A completed mutation: success carries a value; failure carries field feedback. */
export type ActionResult<out T> =
  | {| readonly status: "success", readonly value: T, readonly message: string |}
  | {| readonly status: "error", readonly message: string, readonly fields: FieldErrors |};

/** An action result before or after submission; pending state belongs to React Actions. */
export type FormState<out T> = {| readonly status: "idle" |} | ActionResult<T>;

/** Serializable identity state for rendering, never proof of authorization for a write. */
export type Session =
  | {| readonly kind: "guest" |}
  | {| readonly kind: "authenticated", readonly user: User |};

/** A private read either requires authentication or carries an authorized value. */
export type Protected<out T> =
  | {| readonly kind: "unauthenticated" |}
  | {| readonly kind: "ready", readonly value: T |};

/** Normalized, bounded URL state shared by the loader, search form, and repository. */
export type FeedFilter = {|
  readonly topic: Topic | "all",
  readonly query: string,
  readonly page: number,
|};

/** One feed page and the presence of a next page, without an expensive total count. */
export type FeedData = {|
  ...FeedFilter,
  readonly posts: $ReadOnlyArray<Post>,
  readonly hasNext: boolean,
|};

/** The current account’s authorized conversation previews. */
export type InboxData = Protected<$ReadOnlyArray<MessageThread>>;

/**
 * Distinguish a signed-out reader, an empty inbox, a missing thread, and a ready conversation.
 */
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

/** Construct a successful transport result with its committed server value. */
export function succeeded<T>(value: T, message: string): ActionResult<T> {
  return { status: "success", value, message };
}

/** Construct user-safe failure feedback without admitting a success value. */
export function failed(message: string, fields: FieldErrors = {}): ActionResult<empty> {
  return { status: "error", message, fields };
}

/** Read a field message only from the failed variant of an action state. */
export function fieldError(state: FormState<mixed>, name: string): string | null {
  return match (state) {
    {status: "idle"} | {status: "success", ...} => null,
    {status: "error", fields: const fields, ...} => fields[name] ?? null,
  };
}

/** Normalize untrusted URL values and cap the search length and page offset. */
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

/** Map each stored channel identifier to its visible navigation label. */
export function topicLabel(topic: Topic): string {
  return match (topic) {
    "design" => "Design",
    "release" => "Shipping",
    "runtime" => "Engineering",
    "community" => "Community",
  };
}

/** Validate a channel without silently coercing an unknown input into a writable topic. */
export function topicFrom(value: string): Topic | null {
  return match (value) {
    "design" => "design",
    "release" => "release",
    "runtime" => "runtime",
    "community" => "community",
    _ => null,
  };
}

/** Derive an avatar fallback from at most two words of a public display name. */
export function profileInitials(user: User | Settings): string {
  const name = "displayName" in user ? user.displayName : user.name;

  return name
    .trim()
    .split(/\s+/)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? "")
    .join("");
}

/** Build a canonical feed URL, omitting defaults and encoding search text. */
export function feedHref(topic: Topic | "all", query: string = "", page: number = 1): string {
  const search = new URLSearchParams();
  if (topic !== "all") search.set("topic", topic);
  if (query !== "") search.set("q", query);
  if (page > 1) search.set("page", String(page));

  return search.size === 0 ? "/" : `/?${search.toString()}`;
}

/** Format a UTC calendar date identically during SSR and browser hydration. */
export function displayDate(value: string): string {
  // Explicit UTC keeps server and browser markup identical.

  return new Intl.DateTimeFormat("en", { month: "short", day: "numeric", timeZone: "UTC" }).format(
    new Date(value),
  );
}

/** Format a UTC message time identically during SSR and browser hydration. */
export function displayTime(value: string): string {
  return new Intl.DateTimeFormat("en", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
    timeZone: "UTC",
  }).format(new Date(value));
}

/** Resolve licensed portraits for fixture authors; new accounts retain their initials. */
export function avatarPhoto(id: string): string | null {
  return match (id) {
    "seed-mika" => "/media/avatars/mika.jpg",
    "seed-ren" => "/media/avatars/ren.jpg",
    "seed-sora" => "/media/avatars/sora.jpg",
    "seed-niko" => "/media/avatars/niko.jpg",
    _ => null,
  };
}
