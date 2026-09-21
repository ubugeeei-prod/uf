// @flow
// The web Commonplace examples keep members, notes, reactions, conversations and
// private settings in a service, and reach it over a network. This app has no
// backend, but it keeps the shape: every read and write is a promise that takes
// a moment, because that is what the screens above it have to be written for.
// Nothing here knows about React.

import type {
  Account,
  ActionResult,
  ConversationData,
  FeedFilter,
  Message,
  Post,
  Protected,
  Session,
  Settings,
  Thread,
  Topic,
  User,
} from "./social.js";

import { MAX_MESSAGE_LENGTH, MAX_POST_LENGTH, failed, initials, succeeded } from "./social.js";

/** Everything a screen can ask for. A test may wrap one to delay or refuse a call. */

export type Service = {|
  readonly session: () => Promise<Session>,
  readonly feed: (filter: FeedFilter) => Promise<$ReadOnlyArray<Post>>,
  readonly post: (id: string) => Promise<Post | null>,
  readonly threads: () => Promise<Protected<$ReadOnlyArray<Thread>>>,
  readonly conversation: (thread: string) => Promise<ConversationData>,
  readonly settings: () => Promise<Protected<Settings>>,
  readonly join: (account: Account) => Promise<ActionResult<User>>,
  readonly signOut: () => Promise<ActionResult<null>>,
  readonly publish: (body: string, topic: Topic) => Promise<ActionResult<Post>>,
  readonly appreciate: (id: string, liked: boolean) => Promise<ActionResult<Post>>,
  readonly send: (thread: string, body: string) => Promise<ActionResult<Message>>,
  readonly updateSettings: (settings: Settings) => Promise<ActionResult<Settings>>,
|};

type StoredPost = {|
  readonly id: string,
  readonly authorId: string,
  readonly body: string,
  readonly topic: Topic,
  readonly createdAt: string,
  readonly reactions: $ReadOnlyArray<string>,
|};

type Store = {|
  viewerId: string | null,
  users: { [id: string]: User },
  emails: { [id: string]: string },
  posts: $ReadOnlyArray<StoredPost>,
  messages: { [key: string]: $ReadOnlyArray<Message> },
  serial: number,
|};

const PEOPLE: $ReadOnlyArray<User> = [
  { id: "seed-mika", name: "Mika Tan", handle: "mika", avatar: "M", bio: "Product engineering" },
  { id: "seed-ren", name: "Ren Ito", handle: "ren", avatar: "R", bio: "Design systems" },
  { id: "seed-sora", name: "Sora Lin", handle: "sora", avatar: "S", bio: "Developer tools" },
  { id: "seed-niko", name: "Niko Reyes", handle: "niko", avatar: "N", bio: "Community" },
];

const SEEDED_AT = "2026-09-11T09:40:00.000Z";

const WELCOME = "Welcome to Commonplace. What are you working on?";

const SIGN_IN_FIRST = "Sign in to continue.";

/** The correspondents a new account can write to, as in the web fixture. */

const CORRESPONDENTS: $ReadOnlyArray<string> = ["seed-mika", "seed-sora"];

const NOTES: $ReadOnlyArray<[string, Topic, string]> = [
  [
    "seed-mika",
    "design",
    "Removed the company-size question from signup. We never used the answer, and it was the most common place people dropped off.\n\nThe new flow is in staging if anyone has five minutes to try it.",
  ],
  [
    "seed-ren",
    "release",
    "Reading view is live. J / K moves between notes, and the text-size setting now carries across devices.\n\nStill fixing a selection bug in Safari. Please send me a recording if you hit it.",
  ],
  [
    "seed-sora",
    "runtime",
    "Found the slow query. We were loading every message in a workspace just to show the inbox preview. Down from 840 ms to 46 ms after adding the index and limiting the result.\n\nQuery plan is in the engineering notes.",
  ],
  [
    "seed-niko",
    "community",
    "Anyone using a split keyboard? Considering a Corne, but six keys per thumb seems like a lot to learn at once. Curious how long the adjustment took.",
  ],
];

function seeded(): Store {
  const users: { [id: string]: User } = {};
  for (const person of PEOPLE) users[person.id] = person;

  return {
    viewerId: null,
    users,
    emails: {},
    posts: NOTES.map(([authorId, topic, body], index) => ({
      id: `note-${index}`,
      authorId,
      body,
      topic,
      createdAt: SEEDED_AT,
      reactions: [],
    })),
    messages: {},
    serial: NOTES.length,
  };
}

/** How long a call takes unless the caller says otherwise: long enough to see what waits. */

export const LATENCY: number = 350;

/**
 * A fresh service with the web fixture's members and notes. It resets when the app restarts;
 * `latency` is a parameter so that a test does not spend its time waiting.
 */

export function createService(latency: number = LATENCY): Service {
  const store = seeded();

  const after = <T>(answer: () => T): Promise<T> =>
    new Promise((resolve) => setTimeout(() => resolve(answer()), latency));

  const viewer = (): User | null => (store.viewerId == null ? null : store.users[store.viewerId]);

  const present = (stored: StoredPost): Post => {
    const reader = viewer();

    return {
      id: stored.id,
      author: store.users[stored.authorId],
      body: stored.body,
      topic: stored.topic,
      createdAt: stored.createdAt,
      likes: stored.reactions.length,
      liked: reader != null && stored.reactions.includes(reader.id),
    };
  };

  const log = (reader: User, thread: string): $ReadOnlyArray<Message> => [
    { id: `welcome-${thread}`, author: "them", body: WELCOME, sentAt: "2026-09-11T09:00:00.000Z" },
    ...(store.messages[`${reader.id}:${thread}`] ?? []),
  ];

  const threadsOf = (reader: User): $ReadOnlyArray<Thread> =>
    CORRESPONDENTS.map((id) => {
      const participant = store.users[id];
      const thread = `thread-${participant.handle}`;
      const messages = log(reader, thread);

      return { id: thread, participant, lastMessage: messages[messages.length - 1].body };
    });

  return {
    session: () =>
      after(() => {
        const user = viewer();

        return user == null ? { kind: "guest" } : { kind: "authenticated", user };
      }),

    feed: (filter) =>
      after(() => {
        const wanted = filter.search.trim().toLowerCase();

        return store.posts
          .filter((stored) => filter.topic === "all" || stored.topic === filter.topic)
          .map(present)
          .filter(
            (post) =>
              wanted === "" ||
              post.body.toLowerCase().includes(wanted) ||
              post.author.name.toLowerCase().includes(wanted) ||
              post.author.handle.toLowerCase().includes(wanted),
          );
      }),

    post: (id) =>
      after(() => {
        const stored = store.posts.find((item) => item.id === id);

        return stored == null ? null : present(stored);
      }),

    threads: () =>
      after(() => {
        const reader = viewer();

        return reader == null
          ? { kind: "unauthenticated" }
          : { kind: "ready", value: threadsOf(reader) };
      }),

    conversation: (id) =>
      after(() => {
        const reader = viewer();
        if (reader == null) return { kind: "unauthenticated" };
        const thread = threadsOf(reader).find((item) => item.id === id);

        return thread == null
          ? { kind: "missing" }
          : { kind: "ready", thread, messages: log(reader, thread.id) };
      }),

    settings: () =>
      after(() => {
        const reader = viewer();

        return reader == null
          ? { kind: "unauthenticated" }
          : {
              kind: "ready",
              value: {
                displayName: reader.name,
                bio: reader.bio,
                email: store.emails[reader.id] ?? "",
              },
            };
      }),

    join: (account) =>
      after(() => {
        const name = account.name.trim();
        const handle = account.handle.trim();
        const email = account.email.trim();
        if (name === "" || name.length > 80)
          return failed("Enter a display name of up to 80 characters.");
        if (!/^[a-z][a-z0-9_]{2,23}$/.test(handle))
          return failed(
            "Handles start with a letter and use 3–24 lowercase letters, digits or underscores.",
          );
        if (!/^[^@\s]+@[^@\s]+$/.test(email)) return failed("Enter an email address.");
        if (Object.values(store.users).some((user) => user.handle === handle))
          return failed("That handle is taken.");

        const id = `account-${store.serial}`;
        const user = { id, name, handle, avatar: initials(name), bio: "" };
        store.serial += 1;
        store.users = { ...store.users, [id]: user };
        store.emails = { ...store.emails, [id]: email };
        store.viewerId = id;

        return succeeded(user, "Welcome to Commonplace.");
      }),

    signOut: () =>
      after(() => {
        store.viewerId = null;

        return succeeded(null);
      }),

    publish: (body, topic) =>
      after(() => {
        const author = viewer();
        const text = body.trim();
        if (author == null) return failed(SIGN_IN_FIRST);
        if (text === "") return failed("Write something first.");
        if (text.length > MAX_POST_LENGTH)
          return failed(`Notes are limited to ${MAX_POST_LENGTH} characters.`);

        const stored = {
          id: `note-${store.serial}`,
          authorId: author.id,
          body: text,
          topic,
          createdAt: new Date().toISOString(),
          reactions: [],
        };
        store.serial += 1;
        store.posts = [stored, ...store.posts];

        return succeeded(present(stored), "Published.");
      }),

    appreciate: (id, liked) =>
      after(() => {
        const reader = viewer();
        if (reader == null) return failed(SIGN_IN_FIRST);
        const stored = store.posts.find((item) => item.id === id);
        if (stored == null) return failed("This note is no longer here.");

        const others = stored.reactions.filter((person) => person !== reader.id);
        const next = { ...stored, reactions: liked ? [...others, reader.id] : others };
        store.posts = store.posts.map((item) => (item.id === id ? next : item));

        return succeeded(present(next));
      }),

    send: (thread, body) =>
      after(() => {
        const reader = viewer();
        const text = body.trim();
        if (reader == null) return failed(SIGN_IN_FIRST);
        if (!threadsOf(reader).some((item) => item.id === thread))
          return failed("Conversation not found.");
        if (text === "") return failed("Write a message first.");
        if (text.length > MAX_MESSAGE_LENGTH)
          return failed(`Messages are limited to ${MAX_MESSAGE_LENGTH} characters.`);

        const message: Message = {
          id: `message-${store.serial}`,
          author: "me",
          body: text,
          sentAt: new Date().toISOString(),
        };
        const key = `${reader.id}:${thread}`;
        store.serial += 1;
        store.messages = { ...store.messages, [key]: [...(store.messages[key] ?? []), message] };

        return succeeded(message);
      }),

    updateSettings: (settings) =>
      after(() => {
        const reader = viewer();
        const name = settings.displayName.trim();
        const bio = settings.bio.trim();
        const email = settings.email.trim();
        if (reader == null) return failed(SIGN_IN_FIRST);
        if (name === "" || name.length > 80)
          return failed("Enter a display name of up to 80 characters.");
        if (bio.length > 240) return failed("Bios are limited to 240 characters.");
        if (!/^[^@\s]+@[^@\s]+$/.test(email)) return failed("Enter an email address.");

        store.users = {
          ...store.users,
          [reader.id]: { ...reader, name, bio, avatar: initials(name) },
        };
        store.emails = { ...store.emails, [reader.id]: email };

        return succeeded({ displayName: name, bio, email }, "Your changes are saved.");
      }),
  };
}
