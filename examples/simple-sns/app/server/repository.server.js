// @flow

import { database, transaction, type Row } from "./database.server.js";
import { InputError, identifier } from "./validation.server.js";
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
} from "../social-model.js";

/** Project a database row into the public profile DTO, excluding credentials and email. */
export function publicUser(row: Row): User {
  const user = {
    id: String(row.id),
    name: String(row.name),
    handle: String(row.handle),
    bio: String(row.bio),
    avatar: "",
  };

  return { ...user, avatar: profileInitials(user) };
}

/** Find the public profile for a stable account identifier. */
export function member(id: string): User | null {
  const row = database().prepare("SELECT id, name, handle, bio FROM members WHERE id = ?").get(id);

  return row == null ? null : publicUser(row);
}
const POST_SELECT = `SELECT e.id, e.body, e.topic, e.created_at, m.id AS author_id, m.name, m.handle, m.bio,
  (SELECT count(*) FROM reactions r WHERE r.entry_id=e.id) AS likes,
  EXISTS(SELECT 1 FROM reactions r WHERE r.entry_id=e.id AND r.member_id=?) AS liked
  FROM entries e JOIN members m ON m.id=e.author_id`;

function asPost(row: Row): Post {
  return {
    id: String(row.id),
    body: String(row.body),
    topic: topicFrom(String(row.topic)) ?? "community",
    createdAt: String(row.created_at),
    likes: Number(row.likes),
    liked: Number(row.liked) === 1,
    author: publicUser({ id: row.author_id, name: row.name, handle: row.handle, bio: row.bio }),
  };
}

/**
 * Read one ordered page plus one sentinel row.
 * Search is literal and parameters are bound; reactions are relative to the supplied viewer.
 */
export function listPosts(
  viewerId: string | null,
  topic: Topic | "all",
  query: string,
  page: number,
): Array<Post> {
  // instr is literal search; user input cannot become LIKE wildcards or SQL.

  return database()
    .prepare(
      `${POST_SELECT}
    WHERE (? = 'all' OR e.topic = ?) AND (? = '' OR instr(lower(e.body || ' ' || m.name || ' ' || m.handle), ?) > 0)
    ORDER BY e.created_at DESC, e.id DESC LIMIT ? OFFSET ?`,
    )
    .all(viewerId, topic, topic, query, query.toLowerCase(), PAGE_SIZE + 1, (page - 1) * PAGE_SIZE)
    .map(asPost);
}

function post(id: string, viewerId: string): Post {
  const row = database().prepare(`${POST_SELECT} WHERE e.id = ?`).get(viewerId, id);
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
    const previous = db
      .prepare("SELECT id, body, topic FROM entries WHERE author_id=? AND request_id=?")
      .get(viewer.id, requestId);
    if (previous != null) {
      if (previous.body !== body || previous.topic !== topic)
        throw new InputError("This submission was already used. Please try again.");
      return post(String(previous.id), viewer.id);
    }
    const id = crypto.randomUUID();
    db.prepare("INSERT INTO entries VALUES (?, ?, ?, ?, ?, ?)").run(
      id,
      viewer.id,
      body,
      topic,
      new Date().toISOString(),
      requestId,
    );
    return post(id, viewer.id);
  });
}

/** Set the viewer’s intended reaction state atomically; repeated requests are idempotent. */
export function setReaction(viewer: User, id: string, liked: boolean): Post {
  identifier(id);

  return transaction((db) => {
    post(id, viewer.id);
    if (liked) db.prepare("INSERT OR IGNORE INTO reactions VALUES (?, ?)").run(id, viewer.id);
    else db.prepare("DELETE FROM reactions WHERE entry_id=? AND member_id=?").run(id, viewer.id);
    return post(id, viewer.id);
  });
}

/** Return at most 50 previews for conversations the viewer participates in. */
export function listThreads(viewer: User): Array<MessageThread> {
  return database()
    .prepare(
      `SELECT c.id, m.id AS member_id, m.name, m.handle, m.bio,
    coalesce((SELECT body FROM notes WHERE conversation_id=c.id ORDER BY created_at DESC, id DESC LIMIT 1), '') AS last_message
    FROM conversations c JOIN participants mine ON mine.conversation_id=c.id AND mine.member_id=?
    JOIN participants other ON other.conversation_id=c.id AND other.member_id<>?
    JOIN members m ON m.id=other.member_id ORDER BY c.id LIMIT 50`,
    )
    .all(viewer.id, viewer.id)
    .map((row) => ({
      id: String(row.id),
      name: String(row.name),
      handle: String(row.handle),
      avatar: profileInitials(publicUser(row)),
      photo: avatarPhoto(String(row.member_id)),
      lastMessage: String(row.last_message),
    }));
}

function requireParticipant(viewer: User, threadId: string): void {
  identifier(threadId);
  if (
    database()
      .prepare("SELECT 1 FROM participants WHERE conversation_id=? AND member_id=?")
      .get(threadId, viewer.id) == null
  )
    throw new InputError("This conversation is not available.");
}

function asMessage(row: Row, viewer: User): Message {
  return {
    id: String(row.id),
    threadId: String(row.conversation_id),
    author: row.author_id === viewer.id ? "me" : "them",
    body: String(row.body),
    sentAt: String(row.created_at),
  };
}

/** Verify membership before returning the latest 50 messages in chronological order. */
export function listMessages(viewer: User, threadId: string): Array<Message> {
  requireParticipant(viewer, threadId);

  return database()
    .prepare(
      "SELECT * FROM (SELECT * FROM notes WHERE conversation_id=? ORDER BY created_at DESC, id DESC LIMIT 50) ORDER BY created_at, id",
    )
    .all(threadId)
    .map((row) => asMessage(row, viewer));
}

/**
 * Verify membership and persist one message per author and request ID.
 * A reused ID cannot redirect a previous submission into a different conversation.
 */
export function insertMessage(
  viewer: User,
  threadId: string,
  body: string,
  requestId: string,
): Message {
  return transaction((db) => {
    requireParticipant(viewer, threadId);
    const previous = db
      .prepare("SELECT * FROM notes WHERE author_id=? AND request_id=?")
      .get(viewer.id, requestId);
    if (previous != null) {
      if (previous.conversation_id !== threadId || previous.body !== body)
        throw new InputError("This submission was already used. Please try again.");
      return asMessage(previous, viewer);
    }
    const id = crypto.randomUUID();
    const sentAt = new Date().toISOString();
    db.prepare("INSERT INTO notes VALUES (?, ?, ?, ?, ?, ?)").run(
      id,
      threadId,
      viewer.id,
      body,
      sentAt,
      requestId,
    );
    return { id, threadId, author: "me", body, sentAt };
  });
}

/** Read private profile fields for the already authenticated account. */
export function settingsFor(viewer: User): Settings {
  const row = database()
    .prepare("SELECT name, handle, bio, email FROM members WHERE id=?")
    .get(viewer.id);
  if (row == null) {
    throw new InputError("Please sign in again.");
  }

  return {
    displayName: String(row.name),
    handle: String(row.handle),
    bio: String(row.bio),
    email: String(row.email),
  };
}

/** Check handle uniqueness and update the account in one transaction. */
export function saveSettings(viewer: User, next: Settings): Settings {
  return transaction((db) => {
    if (
      db.prepare("SELECT id FROM members WHERE handle=? AND id<>?").get(next.handle, viewer.id) !=
      null
    )
      throw new InputError("That handle is already taken.", { handle: "Choose another handle." });
    db.prepare("UPDATE members SET name=?, handle=?, bio=?, email=? WHERE id=?").run(
      next.displayName,
      next.handle,
      next.bio,
      next.email,
      viewer.id,
    );
    return settingsFor(viewer);
  });
}

/** Create an account’s private fixture conversation inside the signup transaction. */
export function welcomeConversation(memberId: string): void {
  const db = database();
  const id = crypto.randomUUID();
  db.prepare("INSERT INTO conversations VALUES (?)").run(id);
  db.prepare("INSERT INTO participants VALUES (?, ?), (?, ?)").run(id, memberId, id, "seed-mika");
  db.prepare("INSERT INTO notes VALUES (?, ?, ?, ?, ?, ?)").run(
    crypto.randomUUID(),
    id,
    "seed-mika",
    "This sample conversation belongs to your account. Try sending a message below; it will still be here when you reload.",
    new Date().toISOString(),
    `welcome-${memberId}`,
  );
}
