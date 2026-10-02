-- Commonplace's local store. Applied at startup with CREATE IF NOT EXISTS, and
-- read by sqlc as the catalog for app/_server/db/query.sql.

CREATE TABLE IF NOT EXISTS members (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  handle TEXT NOT NULL UNIQUE,
  bio TEXT NOT NULL DEFAULT '',
  email TEXT NOT NULL DEFAULT '',
  password_hash TEXT
);

CREATE TABLE IF NOT EXISTS entries (
  id TEXT PRIMARY KEY,
  author_id TEXT NOT NULL REFERENCES members(id),
  body TEXT NOT NULL CHECK(length(body) BETWEEN 1 AND 500),
  topic TEXT NOT NULL CHECK(topic IN ('design', 'release', 'runtime', 'community')),
  created_at TEXT NOT NULL,
  request_id TEXT NOT NULL,
  UNIQUE(author_id, request_id)
);

CREATE INDEX IF NOT EXISTS entries_newest ON entries(created_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS reactions (
  entry_id TEXT NOT NULL REFERENCES entries(id),
  member_id TEXT NOT NULL REFERENCES members(id),
  PRIMARY KEY(entry_id, member_id)
);

CREATE TABLE IF NOT EXISTS conversations (
  id TEXT PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS participants (
  conversation_id TEXT NOT NULL REFERENCES conversations(id),
  member_id TEXT NOT NULL REFERENCES members(id),
  PRIMARY KEY(conversation_id, member_id)
);

CREATE TABLE IF NOT EXISTS notes (
  id TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL REFERENCES conversations(id),
  author_id TEXT NOT NULL REFERENCES members(id),
  body TEXT NOT NULL CHECK(length(body) BETWEEN 1 AND 2000),
  created_at TEXT NOT NULL,
  request_id TEXT NOT NULL,
  UNIQUE(author_id, request_id)
);

CREATE INDEX IF NOT EXISTS notes_conversation ON notes(conversation_id, created_at, id);

CREATE TABLE IF NOT EXISTS sessions (
  token_hash TEXT PRIMARY KEY,
  member_id TEXT NOT NULL REFERENCES members(id),
  expires_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS auth_attempts (
  handle TEXT PRIMARY KEY,
  attempts INTEGER NOT NULL,
  resets_at INTEGER NOT NULL
);
