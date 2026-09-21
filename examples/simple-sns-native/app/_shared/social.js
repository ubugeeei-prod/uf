// @flow
// The web Commonplace examples keep this state in a service: members, notes,
// reactions, conversations and the account's private settings. This app has no
// backend, so the same model lives in memory behind one provider, seeded with
// the same members and notes, and resets when the app restarts.

import * as React from "react";

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
  readonly messages: $ReadOnlyArray<Message>,
|};

/** Private profile fields, shown only to the account that owns them. */

export type Settings = {|
  readonly displayName: string,
  readonly bio: string,
  readonly email: string,
|};

/** A completed write: either it happened, or the person is told why not. */

export type Outcome = {| readonly ok: true |} | {| readonly ok: false, readonly message: string |};

export const TOPICS: $ReadOnlyArray<Topic> = ["design", "release", "runtime", "community"];

export const MAX_POST_LENGTH: number = 500;

export const MAX_MESSAGE_LENGTH: number = 2000;

/** Map each stored channel identifier to its visible label. */

export function topicLabel(topic: Topic): string {
  if (topic === "design") return "Design";
  if (topic === "release") return "Shipping";
  if (topic === "runtime") return "Engineering";
  return "Community";
}

/** Format a UTC calendar date, the way the web examples print a note's day. */

export function displayDate(value: string): string {
  return new Intl.DateTimeFormat("en", { month: "short", day: "numeric", timeZone: "UTC" }).format(
    new Date(value),
  );
}

/** Format a UTC message time. */

export function displayTime(value: string): string {
  return new Intl.DateTimeFormat("en", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
    timeZone: "UTC",
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

type StoredPost = {|
  readonly id: string,
  readonly authorId: string,
  readonly body: string,
  readonly topic: Topic,
  readonly createdAt: string,
  readonly reactions: $ReadOnlyArray<string>,
|};

type State = {|
  readonly viewerId: string | null,
  readonly users: { readonly [id: string]: User },
  readonly emails: { readonly [id: string]: string },
  readonly posts: $ReadOnlyArray<StoredPost>,
  readonly messages: { readonly [key: string]: $ReadOnlyArray<Message> },
  readonly serial: number,
|};

const PEOPLE: $ReadOnlyArray<User> = [
  { id: "seed-mika", name: "Mika Tan", handle: "mika", avatar: "M", bio: "Product engineering" },
  { id: "seed-ren", name: "Ren Ito", handle: "ren", avatar: "R", bio: "Design systems" },
  { id: "seed-sora", name: "Sora Lin", handle: "sora", avatar: "S", bio: "Developer tools" },
  { id: "seed-niko", name: "Niko Reyes", handle: "niko", avatar: "N", bio: "Community" },
];

const SEEDED_AT = "2026-09-11T09:40:00.000Z";

const WELCOME = "Welcome to Commonplace. What are you working on?";

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

function seeded(): State {
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

export type Account = {|
  readonly name: string,
  readonly handle: string,
  readonly email: string,
|};

type Social = {|
  readonly viewer: User | null,
  readonly settings: Settings | null,
  readonly people: $ReadOnlyArray<User>,
  readonly feed: (topic: TopicFilter, search: string) => $ReadOnlyArray<Post>,
  readonly post: (id: string) => Post | null,
  readonly threads: $ReadOnlyArray<Thread>,
  readonly join: (account: Account) => Outcome,
  readonly signOut: () => void,
  readonly publish: (body: string, topic: Topic) => Outcome,
  readonly appreciate: (id: string) => void,
  readonly send: (thread: string, body: string) => Outcome,
  readonly updateSettings: (settings: Settings) => Outcome,
|};

const Context: React.Context<Social | null> = React.createContext(null);

const DONE: Outcome = { ok: true };

const refused = (message: string): Outcome => ({ ok: false, message });

const SIGN_IN_FIRST = "Sign in to continue.";

/** One in-memory service for the whole app; every screen reads and writes through it. */

export component SocialProvider(children: React.Node) {
  const [state, setState] = React.useState(seeded);
  const viewer = state.viewerId == null ? null : state.users[state.viewerId];

  const present = (stored: StoredPost): Post => ({
    id: stored.id,
    author: state.users[stored.authorId],
    body: stored.body,
    topic: stored.topic,
    createdAt: stored.createdAt,
    likes: stored.reactions.length,
    liked: viewer != null && stored.reactions.includes(viewer.id),
  });

  const social: Social = {
    viewer,
    settings:
      viewer == null
        ? null
        : { displayName: viewer.name, bio: viewer.bio, email: state.emails[viewer.id] ?? "" },
    people: PEOPLE,
    feed: (topic, search) => {
      const wanted = search.trim().toLowerCase();

      return state.posts
        .filter((stored) => topic === "all" || stored.topic === topic)
        .map(present)
        .filter(
          (post) =>
            wanted === "" ||
            post.body.toLowerCase().includes(wanted) ||
            post.author.name.toLowerCase().includes(wanted) ||
            post.author.handle.toLowerCase().includes(wanted),
        );
    },
    post: (id) => {
      const stored = state.posts.find((item) => item.id === id);

      return stored == null ? null : present(stored);
    },
    threads:
      viewer == null
        ? []
        : CORRESPONDENTS.map((id) => {
            const participant = state.users[id];
            const thread = `thread-${participant.handle}`;
            const messages: $ReadOnlyArray<Message> = [
              {
                id: `welcome-${thread}`,
                author: "them",
                body: WELCOME,
                sentAt: "2026-09-11T09:00:00.000Z",
              },
              ...(state.messages[`${viewer.id}:${thread}`] ?? []),
            ];

            return {
              id: thread,
              participant,
              lastMessage: messages[messages.length - 1].body,
              messages,
            };
          }),
    join: (account) => {
      const name = account.name.trim();
      const handle = account.handle.trim();
      const email = account.email.trim();
      if (name === "" || name.length > 80)
        return refused("Enter a display name of up to 80 characters.");
      if (!/^[a-z][a-z0-9_]{2,23}$/.test(handle))
        return refused(
          "Handles start with a letter and use 3–24 lowercase letters, digits or underscores.",
        );
      if (!/^[^@\s]+@[^@\s]+$/.test(email)) return refused("Enter an email address.");
      if (Object.values(state.users).some((user) => user.handle === handle))
        return refused("That handle is taken.");

      setState((current) => {
        const id = `account-${current.serial}`;

        return {
          ...current,
          viewerId: id,
          users: { ...current.users, [id]: { id, name, handle, avatar: initials(name), bio: "" } },
          emails: { ...current.emails, [id]: email },
          serial: current.serial + 1,
        };
      });

      return DONE;
    },
    signOut: () => setState((current) => ({ ...current, viewerId: null })),
    publish: (body, topic) => {
      const text = body.trim();
      if (viewer == null) return refused(SIGN_IN_FIRST);
      if (text === "") return refused("Write something first.");
      if (text.length > MAX_POST_LENGTH)
        return refused(`Notes are limited to ${MAX_POST_LENGTH} characters.`);

      setState((current) => ({
        ...current,
        posts: [
          {
            id: `note-${current.serial}`,
            authorId: viewer.id,
            body: text,
            topic,
            createdAt: new Date().toISOString(),
            reactions: [],
          },
          ...current.posts,
        ],
        serial: current.serial + 1,
      }));

      return DONE;
    },
    appreciate: (id) => {
      if (viewer == null) return;

      setState((current) => ({
        ...current,
        posts: current.posts.map((stored) =>
          stored.id !== id
            ? stored
            : {
                ...stored,
                reactions: stored.reactions.includes(viewer.id)
                  ? stored.reactions.filter((reader) => reader !== viewer.id)
                  : [...stored.reactions, viewer.id],
              },
        ),
      }));
    },
    send: (thread, body) => {
      const text = body.trim();
      if (viewer == null) return refused(SIGN_IN_FIRST);
      if (text === "") return refused("Write a message first.");
      if (text.length > MAX_MESSAGE_LENGTH)
        return refused(`Messages are limited to ${MAX_MESSAGE_LENGTH} characters.`);

      setState((current) => {
        const key = `${viewer.id}:${thread}`;

        return {
          ...current,
          messages: {
            ...current.messages,
            [key]: [
              ...(current.messages[key] ?? []),
              {
                id: `message-${current.serial}`,
                author: "me",
                body: text,
                sentAt: new Date().toISOString(),
              },
            ],
          },
          serial: current.serial + 1,
        };
      });

      return DONE;
    },
    updateSettings: (settings) => {
      const name = settings.displayName.trim();
      const bio = settings.bio.trim();
      const email = settings.email.trim();
      if (viewer == null) return refused(SIGN_IN_FIRST);
      if (name === "" || name.length > 80)
        return refused("Enter a display name of up to 80 characters.");
      if (bio.length > 240) return refused("Bios are limited to 240 characters.");
      if (!/^[^@\s]+@[^@\s]+$/.test(email)) return refused("Enter an email address.");

      setState((current) => ({
        ...current,
        users: {
          ...current.users,
          [viewer.id]: { ...viewer, name, bio, avatar: initials(name) },
        },
        emails: { ...current.emails, [viewer.id]: email },
      }));

      return DONE;
    },
  };

  return <Context.Provider value={social}>{children}</Context.Provider>;
}

export function useSocial(): Social {
  const social = React.useContext(Context);
  if (social == null) throw new Error("SocialProvider is missing");

  return social;
}
