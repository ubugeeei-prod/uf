// @flow

import type { SyncQueryable } from "@uniflowed/sql";

import { database, transaction } from "./database.server.js";
import { InputError, identifier } from "./validation.server.js";
import {
  deleteReaction,
  entryByRequest,
  getMember,
  getPost,
  handleOwner,
  insertConversation,
  insertEntry,
  insertNote,
  insertParticipant,
  insertReaction,
  listMessages as selectMessages,
  listPosts as selectPosts,
  listThreads as selectThreads,
  memberSettings,
  noteByRequest,
  participant,
  updateMember,
  type GetPostRow,
  type ListMessagesRow,
  type ListPostsRow,
} from "./db/query.sql.js";
import type { Note } from "./db/models.js";
import {
  PAGE_SIZE,
  avatarPhoto,
  profileInitials,
  topicFrom,
  type User,
  type Post,
  type Topic,
  type Settings,
  type Message,
  type MessageThread,
} from "../_shared/social-model.js";

type Profile = {
  readonly id    : string,
  readonly name  : string,
  readonly handle: string,
  readonly bio   : string,
  ...
};

/** Project a member row into the public profile DTO, excluding credentials and email. */

export function publicUser(row: Profile): User {
  const user = {
    id    : row.id,
    name  : row.name,
    handle: row.handle,
    bio   : row.bio,
    avatar: "",
  };

  return { ...user, avatar: profileInitials(user) };
}

/** Find the public profile for a stable account identifier. */

export function member(id: string, db: SyncQueryable = database()): User | null {
  const row = getMember(db, { id });

  return row == null ? null : publicUser(row);
}

function asPost(row: ListPostsRow | GetPostRow): Post {
  return {
    id       : row.id,
    body     : row.body,
    topic    : topicFrom(row.topic) ?? "community",
    createdAt: row.createdAt,
    likes    : row.likes,
    liked    : row.liked,
    author: publicUser({
      id    : row.authorId,
      name  : row.name,
      handle: row.handle,
      bio   : row.bio,
    }),
  };
}

/**
 * Read one ordered page plus one sentinel row.
 * Search is literal and parameters are bound; reactions are relative to the supplied viewer.
 */

export function listPosts(
  viewerId: string | null,
  topic   : Topic | "all",
  query   : string,
  page    : number,
): Array<Post> {
  // instr is literal search; user input cannot become LIKE wildcards or SQL.
  // An empty needle matches every row. "all" leaves the topic unconstrained.

  return selectPosts(database(), {
    viewerId,
    topic : topic === "all" ? null : topic,
    needle: query.toLowerCase(),
    limit : PAGE_SIZE + 1,
    offset: (page - 1) * PAGE_SIZE,
  }).map(asPost);
}

function post(id: string, viewerId: string, db: SyncQueryable): Post {
  const row = getPost(db, { viewerId, id });
  if (row == null) {
    throw new InputError("This post is no longer available.");
  }

  return asPost(row);
}

/**
 * Publish once per author and request ID within a transaction.
 * An identical retry returns the original note; reusing the ID for different input is rejected.
 */

export function insertPost(viewer: User, body: string, topic: Topic, requestId: string): Post {
  return transaction((db) => {
    const previous = entryByRequest(db, { authorId: viewer.id, requestId });
    if (previous != null) {
      if (previous.body !== body || previous.topic !== topic) {
        throw new InputError("This submission was already used. Please try again.");
      }

      return post(previous.id, viewer.id, db);
    }
    const id = crypto.randomUUID();
    insertEntry(db, {
      id,
      authorId: viewer.id,
      body,
      topic,
      createdAt: new Date().toISOString(),
      requestId,
    });

    return post(id, viewer.id, db);
  });
}

/** Set the viewer’s intended reaction state atomically; repeated requests are idempotent. */

export function setReaction(viewer: User, id: string, liked: boolean): Post {
  identifier(id);

  return transaction((db) => {
    post(id, viewer.id, db);
    if (liked) {
      insertReaction(db, { entryId: id, memberId: viewer.id });
    } else {
      deleteReaction(db, { entryId: id, memberId: viewer.id });
    }

    return post(id, viewer.id, db);
  });
}

/** Return at most 50 previews for conversations the viewer participates in. */

export function listThreads(viewer: User): Array<MessageThread> {
  return selectThreads(database(), { viewerId: viewer.id }).map((row) => ({
    id    : row.id,
    name  : row.name,
    handle: row.handle,
    avatar: profileInitials(
      publicUser({ id: row.memberId, name: row.name, handle: row.handle, bio: row.bio }),
    ),
    photo      : avatarPhoto(row.memberId),
    lastMessage: row.lastMessage,
  }));
}

function requireParticipant(viewer: User, threadId: string, db: SyncQueryable = database()): void {
  identifier(threadId);
  if (participant(db, { conversationId: threadId, memberId: viewer.id }) == null) {
    throw new InputError("This conversation is not available.");
  }
}

function asMessage(row: ListMessagesRow | Note, viewer: User): Message {
  return {
    id      : row.id,
    threadId: row.conversationId,
    author  : row.authorId === viewer.id ? "me" : "them",
    body    : row.body,
    sentAt  : row.createdAt,
  };
}

/** Verify membership before returning the latest 50 messages in chronological order. */

export function listMessages(viewer: User, threadId: string): Array<Message> {
  requireParticipant(viewer, threadId);

  return selectMessages(database(), { conversationId: threadId }).map((row) =>
    asMessage(row, viewer),
  );
}

/**
 * Verify membership and persist one message per author and request ID.
 * A reused ID cannot redirect a previous submission into a different conversation.
 */

export function insertMessage(
  viewer   : User,
  threadId : string,
  body     : string,
  requestId: string,
): Message {
  return transaction((db) => {
    requireParticipant(viewer, threadId, db);
    const previous = noteByRequest(db, { authorId: viewer.id, requestId });
    if (previous != null) {
      if (previous.conversationId !== threadId || previous.body !== body) {
        throw new InputError("This submission was already used. Please try again.");
      }

      return asMessage(previous, viewer);
    }
    const id = crypto.randomUUID();
    const sentAt = new Date().toISOString();
    insertNote(db, {
      id,
      conversationId: threadId,
      authorId      : viewer.id,
      body,
      createdAt: sentAt,
      requestId,
    });

    return { id, threadId, author: "me", body, sentAt };
  });
}

/** Read private profile fields for the already authenticated account. */

export function settingsFor(viewer: User, db: SyncQueryable = database()): Settings {
  const row = memberSettings(db, { id: viewer.id });
  if (row == null) {
    throw new InputError("Please sign in again.");
  }

  return {
    displayName: row.name,
    handle     : row.handle,
    bio        : row.bio,
    email      : row.email,
  };
}

/** Check handle uniqueness and update the account in one transaction. */

export function saveSettings(viewer: User, next: Settings): Settings {
  return transaction((db) => {
    if (handleOwner(db, { handle: next.handle, id: viewer.id }) != null) {
      throw new InputError("That handle is already taken.", { handle: "Choose another handle." });
    }
    updateMember(db, {
      name  : next.displayName,
      handle: next.handle,
      bio   : next.bio,
      email : next.email,
      id    : viewer.id,
    });

    return settingsFor(viewer, db);
  });
}

/** Create an account’s private fixture conversation inside the signup transaction. */

export function welcomeConversation(db: SyncQueryable, memberId: string): void {
  const id = crypto.randomUUID();
  insertConversation(db, { id });
  insertParticipant(db, { conversationId: id, memberId });
  insertParticipant(db, { conversationId: id, memberId: "seed-mika" });
  insertNote(db, {
    id: crypto.randomUUID(),
    conversationId: id,
    authorId: "seed-mika",
    body: "This sample conversation belongs to your account. Try sending a message below; it will still be here when you reload.",
    createdAt: new Date().toISOString(),
    requestId: `welcome-${memberId}`,
  });
}
