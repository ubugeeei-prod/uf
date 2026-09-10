// @flow

import fs from "node:fs";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";

import {
  clampMessageBody,
  clampPostBody,
  normalizeHandle,
  statsFor,
  type FeedStats,
  type Message,
  type MessageThread,
  type Post,
  type Settings,
  type Topic,
  type User,
} from "./social-model.js";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(HERE, "..");
const DB_PATH = process.env.UF_SIMPLE_SNS_DB ?? path.join(APP_ROOT, ".uf", "simple-sns.sqlite");

let instance: DatabaseSync | null = null;

function database(): DatabaseSync {
  if (instance == null) {
    fs.mkdirSync(path.dirname(DB_PATH), { recursive: true });
    instance = new DatabaseSync(DB_PATH);
    instance.exec("PRAGMA foreign_keys = ON");
    migrate(instance);
    seed(instance);
  }
  return instance;
}

function migrate(db: DatabaseSync) {
  db.exec(`
    CREATE TABLE IF NOT EXISTS users (
      id TEXT PRIMARY KEY,
      name TEXT NOT NULL,
      handle TEXT NOT NULL UNIQUE,
      avatar TEXT NOT NULL,
      bio TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS posts (
      id TEXT PRIMARY KEY,
      author_id TEXT NOT NULL REFERENCES users(id),
      body TEXT NOT NULL,
      topic TEXT NOT NULL,
      likes INTEGER NOT NULL,
      replies INTEGER NOT NULL,
      created_at TEXT NOT NULL,
      liked INTEGER NOT NULL DEFAULT 0
    );

    CREATE TABLE IF NOT EXISTS message_threads (
      id TEXT PRIMARY KEY,
      name TEXT NOT NULL,
      handle TEXT NOT NULL,
      unread INTEGER NOT NULL,
      last_message TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS messages (
      id TEXT PRIMARY KEY,
      thread_id TEXT NOT NULL REFERENCES message_threads(id),
      author TEXT NOT NULL,
      body TEXT NOT NULL,
      sent_at TEXT NOT NULL,
      delivery TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS settings (
      id TEXT PRIMARY KEY,
      display_name TEXT NOT NULL,
      handle TEXT NOT NULL,
      bio TEXT NOT NULL,
      email TEXT NOT NULL,
      digest INTEGER NOT NULL,
      quiet_mode INTEGER NOT NULL
    );
  `);
}

function seed(db: DatabaseSync) {
  db.exec(`
    INSERT OR IGNORE INTO users (id, name, handle, avatar, bio) VALUES
      ('u-mika', 'Mika Tan', 'mika', 'MT', 'Builds product loops and keeps the release train boring.'),
      ('u-ren', 'Ren Ito', 'ren', 'RI', 'Designs the calm parts of busy systems.'),
      ('u-sora', 'Sora Lin', 'sora', 'SL', 'Runtime engineer with a soft spot for tiny abstractions.'),
      ('u-niko', 'Niko Reyes', 'niko', 'NR', 'Writes community notes and asks sharp questions.');

    INSERT OR IGNORE INTO posts
      (id, author_id, body, topic, likes, replies, created_at, liked)
    VALUES
      (
        'release-train',
        'u-mika',
        'Cut alpha.19 after the package-manager fix lands. The plan is small, reversible releases with receipts in each changelog.',
        'release',
        22,
        5,
        '2026-09-10T08:20:00.000Z',
        1
      ),
      (
        'runtime-note',
        'u-sora',
        'Server Actions now stay server-colored all the way through the graph. The client sees a reference; the database handle stays home.',
        'runtime',
        31,
        7,
        '2026-09-10T07:35:00.000Z',
        0
      ),
      (
        'design-system',
        'u-ren',
        'Trying a denser shell for examples: less landing-page, more actual product surface. StyleX tokens carry the visual rhythm.',
        'design',
        18,
        3,
        '2026-09-10T06:50:00.000Z',
        0
      ),
      (
        'community-pulse',
        'u-niko',
        'The next docs pass should show how to swap React and hosting adapters without treating uf as the center of the universe.',
        'community',
        16,
        4,
        '2026-09-10T06:10:00.000Z',
        1
      );

    INSERT OR IGNORE INTO message_threads (id, name, handle, unread, last_message) VALUES
      ('thread-ren', 'Ren Ito', 'ren', 2, 'Can the settings surface use the same action state pattern?'),
      ('thread-sora', 'Sora Lin', 'sora', 0, 'The sqlite fixture is ready for browser testing.'),
      ('thread-niko', 'Niko Reyes', 'niko', 1, 'I added release notes questions to the thread.');

    INSERT OR IGNORE INTO messages
      (id, thread_id, author, body, sent_at, delivery)
    VALUES
      ('m-ren-1', 'thread-ren', 'them', 'Can the settings surface use the same action state pattern?', '2026-09-10T08:05:00.000Z', 'read'),
      ('m-ren-2', 'thread-ren', 'me', 'Yes. The form posts to a server action and keeps optimistic client state tiny.', '2026-09-10T08:07:00.000Z', 'read'),
      ('m-sora-1', 'thread-sora', 'them', 'The sqlite fixture is ready for browser testing.', '2026-09-10T07:15:00.000Z', 'read'),
      ('m-sora-2', 'thread-sora', 'me', 'Great. I will keep the module server-only and leave client islands with plain data.', '2026-09-10T07:17:00.000Z', 'delivered'),
      ('m-niko-1', 'thread-niko', 'them', 'I added release notes questions to the thread.', '2026-09-10T06:42:00.000Z', 'read');

    INSERT OR IGNORE INTO settings
      (id, display_name, handle, bio, email, digest, quiet_mode)
    VALUES
      (
        'viewer',
        'Mika Tan',
        'mika',
        'Builds product loops and keeps the release train boring.',
        'mika@example.test',
        1,
        0
      );
  `);
}

function asUser(row: any): User {
  return {
    id: String(row.id),
    name: String(row.name),
    handle: String(row.handle),
    avatar: String(row.avatar),
    bio: String(row.bio),
  };
}

function asPost(row: any): Post {
  return {
    id: String(row.id),
    author: {
      id: String(row.author_id),
      name: String(row.name),
      handle: String(row.handle),
      avatar: String(row.avatar),
      bio: String(row.bio),
    },
    body: String(row.body),
    topic: (String(row.topic): any),
    likes: Number(row.likes),
    replies: Number(row.replies),
    createdAt: String(row.created_at),
    liked: Number(row.liked) === 1,
  };
}

function asThread(row: any): MessageThread {
  return {
    id: String(row.id),
    name: String(row.name),
    handle: String(row.handle),
    unread: Number(row.unread),
    lastMessage: String(row.last_message),
  };
}

function asMessage(row: any): Message {
  return {
    id: String(row.id),
    threadId: String(row.thread_id),
    author: (String(row.author): any),
    body: String(row.body),
    sentAt: String(row.sent_at),
    delivery: (String(row.delivery): any),
  };
}

export async function getViewer(): Promise<User> {
  const row = database().prepare("SELECT * FROM users WHERE id = ?").get("u-mika");
  return asUser(row);
}

export async function listPosts(): Promise<Array<Post>> {
  const rows = database()
    .prepare(
      `
      SELECT
        posts.*,
        users.name,
        users.handle,
        users.avatar,
        users.bio
      FROM posts
      INNER JOIN users ON users.id = posts.author_id
      ORDER BY posts.created_at DESC
    `,
    )
    .all();
  return rows.map(asPost);
}

export async function feedStats(): Promise<FeedStats> {
  return statsFor(await listPosts());
}

export async function listThreads(): Promise<Array<MessageThread>> {
  const rows = database()
    .prepare("SELECT * FROM message_threads ORDER BY unread DESC, name ASC")
    .all();
  return rows.map(asThread);
}

export async function listMessages(threadId: string): Promise<Array<Message>> {
  const rows = database()
    .prepare("SELECT * FROM messages WHERE thread_id = ? ORDER BY sent_at ASC")
    .all(threadId);
  return rows.map(asMessage);
}

export async function getSettings(): Promise<Settings> {
  const row = database().prepare("SELECT * FROM settings WHERE id = ?").get("viewer");
  return {
    displayName: String(row.display_name),
    handle: String(row.handle),
    bio: String(row.bio),
    email: String(row.email),
    digest: Number(row.digest) === 1,
    quietMode: Number(row.quiet_mode) === 1,
  };
}

export async function insertPost(body: string, topic: Topic): Promise<Post> {
  const id = `post-${crypto.randomUUID()}`;
  const cleanBody = clampPostBody(body);
  const now = new Date().toISOString();
  database()
    .prepare(
      `
      INSERT INTO posts (id, author_id, body, topic, likes, replies, created_at, liked)
      VALUES (?, 'u-mika', ?, ?, 1, 0, ?, 1)
    `,
    )
    .run(id, cleanBody, topic, now);
  const posts = await listPosts();
  const post = posts.find((candidate) => candidate.id === id);
  if (post == null) {
    throw new Error("created post was not readable");
  }
  return post;
}

export async function likePostById(id: string): Promise<number> {
  database().prepare("UPDATE posts SET likes = likes + 1, liked = 1 WHERE id = ?").run(id);
  const row = database().prepare("SELECT likes FROM posts WHERE id = ?").get(id);
  return row == null ? 0 : Number(row.likes);
}

export async function insertMessage(threadId: string, body: string): Promise<Message> {
  const id = `msg-${crypto.randomUUID()}`;
  const cleanBody = clampMessageBody(body);
  const now = new Date().toISOString();
  database()
    .prepare(
      `
      INSERT INTO messages (id, thread_id, author, body, sent_at, delivery)
      VALUES (?, ?, 'me', ?, ?, 'sent')
    `,
    )
    .run(id, threadId, cleanBody, now);
  database()
    .prepare("UPDATE message_threads SET last_message = ?, unread = 0 WHERE id = ?")
    .run(cleanBody, threadId);
  return {
    id,
    threadId,
    author: "me",
    body: cleanBody,
    sentAt: now,
    delivery: "sent",
  };
}

export async function saveSettings(next: Settings): Promise<Settings> {
  const handle = normalizeHandle(next.handle);
  database()
    .prepare(
      `
      UPDATE settings
      SET display_name = ?, handle = ?, bio = ?, email = ?, digest = ?, quiet_mode = ?
      WHERE id = 'viewer'
    `,
    )
    .run(
      next.displayName.trim(),
      handle,
      next.bio.trim(),
      next.email.trim(),
      next.digest ? 1 : 0,
      next.quietMode ? 1 : 0,
    );
  return getSettings();
}

export async function upsertDemoUser(
  name: string,
  handleInput: string,
  bio: string,
): Promise<User> {
  const handle = normalizeHandle(handleInput);
  const id = `u-${handle}`;
  const displayName = name.trim().slice(0, 80) || handle;
  const avatar = displayName
    .split(/\s+/)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? "")
    .join("");
  database()
    .prepare("INSERT OR REPLACE INTO users (id, name, handle, avatar, bio) VALUES (?, ?, ?, ?, ?)")
    .run(id, displayName, handle, avatar, bio.trim().slice(0, 160));
  return { id, name: displayName, handle, avatar, bio: bio.trim().slice(0, 160) };
}
