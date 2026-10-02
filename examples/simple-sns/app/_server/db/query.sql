-- name: GetMember :one
SELECT id, name, handle, bio
FROM members
WHERE id = ?;

-- name: ListPosts :many
SELECT
  e.id,
  e.body,
  e.topic,
  e.created_at,
  m.id AS author_id,
  m.name,
  m.handle,
  m.bio,
  (SELECT count(*) FROM reactions r WHERE r.entry_id = e.id) AS likes,
  EXISTS(
    SELECT 1 FROM reactions r
    WHERE r.entry_id = e.id AND r.member_id = sqlc.narg('viewerId')
  ) AS liked
FROM entries e
JOIN members m ON m.id = e.author_id
WHERE e.topic = coalesce(sqlc.narg('topic'), e.topic)
  AND (
    cast(sqlc.arg('needle') AS TEXT) = ''
    OR instr(
      lower(e.body || ' ' || m.name || ' ' || m.handle),
      cast(sqlc.arg('needle') AS TEXT)
    ) > 0
  )
ORDER BY e.created_at DESC, e.id DESC
LIMIT sqlc.arg('limit') OFFSET sqlc.arg('offset');

-- name: GetPost :one
SELECT
  e.id,
  e.body,
  e.topic,
  e.created_at,
  m.id AS author_id,
  m.name,
  m.handle,
  m.bio,
  (SELECT count(*) FROM reactions r WHERE r.entry_id = e.id) AS likes,
  EXISTS(
    SELECT 1 FROM reactions r
    WHERE r.entry_id = e.id AND r.member_id = sqlc.narg('viewerId')
  ) AS liked
FROM entries e
JOIN members m ON m.id = e.author_id
WHERE e.id = sqlc.arg('id');

-- name: EntryByRequest :one
SELECT id, body, topic
FROM entries
WHERE author_id = ? AND request_id = ?;

-- name: InsertEntry :exec
INSERT INTO entries (id, author_id, body, topic, created_at, request_id)
VALUES (?, ?, ?, ?, ?, ?);

-- name: InsertReaction :exec
INSERT OR IGNORE INTO reactions (entry_id, member_id)
VALUES (?, ?);

-- name: DeleteReaction :exec
DELETE FROM reactions
WHERE entry_id = ? AND member_id = ?;

-- name: ListThreads :many
SELECT
  c.id,
  m.id AS member_id,
  m.name,
  m.handle,
  m.bio,
  cast(
    coalesce(
      (SELECT body FROM notes WHERE conversation_id = c.id ORDER BY created_at DESC, id DESC LIMIT 1),
      ''
    ) AS TEXT
  ) AS last_message
FROM conversations c
JOIN participants mine ON mine.conversation_id = c.id AND mine.member_id = sqlc.arg('viewerId')
JOIN participants other ON other.conversation_id = c.id AND other.member_id <> sqlc.arg('viewerId')
JOIN members m ON m.id = other.member_id
ORDER BY c.id
LIMIT 50;

-- name: Participant :one
SELECT member_id
FROM participants
WHERE conversation_id = ? AND member_id = ?;

-- name: ListMessages :many
SELECT id, conversation_id, author_id, body, created_at
FROM (
  SELECT id, conversation_id, author_id, body, created_at
  FROM notes
  WHERE conversation_id = ?
  ORDER BY created_at DESC, id DESC
  LIMIT 50
)
ORDER BY created_at, id;

-- name: NoteByRequest :one
SELECT id, conversation_id, author_id, body, created_at, request_id
FROM notes
WHERE author_id = ? AND request_id = ?;

-- name: InsertNote :exec
INSERT INTO notes (id, conversation_id, author_id, body, created_at, request_id)
VALUES (?, ?, ?, ?, ?, ?);

-- name: MemberSettings :one
SELECT name, handle, bio, email
FROM members
WHERE id = ?;

-- name: HandleOwner :one
SELECT id
FROM members
WHERE handle = ? AND id <> ?;

-- name: UpdateMember :exec
UPDATE members
SET name = ?, handle = ?, bio = ?, email = ?
WHERE id = ?;

-- name: InsertConversation :exec
INSERT INTO conversations (id)
VALUES (?);

-- name: InsertParticipant :exec
INSERT INTO participants (conversation_id, member_id)
VALUES (?, ?);

-- name: SeedMember :exec
INSERT OR IGNORE INTO members (id, name, handle, bio)
VALUES (?, ?, ?, ?);

-- name: SeedEntry :exec
INSERT OR IGNORE INTO entries (id, author_id, body, topic, created_at, request_id)
VALUES (?, ?, ?, ?, ?, ?);

-- name: SeedReaction :exec
INSERT OR IGNORE INTO reactions (entry_id, member_id)
VALUES (?, ?);

-- name: SessionMember :one
SELECT member_id
FROM sessions
WHERE token_hash = ? AND expires_at > ?;

-- name: DeleteExpiredAttempts :exec
DELETE FROM auth_attempts
WHERE resets_at < ?;

-- name: RecordAttempt :exec
INSERT INTO auth_attempts (handle, attempts, resets_at)
VALUES (?, 1, ?)
ON CONFLICT(handle) DO UPDATE SET attempts = attempts + 1;

-- name: AttemptCount :one
SELECT attempts
FROM auth_attempts
WHERE handle = ?;

-- name: MemberByHandle :one
SELECT id, name, handle, bio, password_hash
FROM members
WHERE handle = ?;

-- name: InsertMember :exec
INSERT INTO members (id, name, handle, email, password_hash)
VALUES (?, ?, ?, ?, ?);

-- name: DeleteAttempts :exec
DELETE FROM auth_attempts
WHERE handle = ?;

-- name: DeleteSession :exec
DELETE FROM sessions
WHERE token_hash = ?;

-- name: DeleteExpiredSessions :exec
DELETE FROM sessions
WHERE expires_at <= ?;

-- name: InsertSession :exec
INSERT INTO sessions (token_hash, member_id, expires_at)
VALUES (?, ?, ?);
