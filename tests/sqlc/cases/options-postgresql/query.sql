-- name: GetAccount :one
SELECT * FROM accounts WHERE id = $1;

-- name: FindByExternal :one
SELECT id, created_at FROM accounts WHERE external_id = $1;

-- name: CreateAccount :one
INSERT INTO accounts (external_id, settings, spotify_url) VALUES ($1, $2, $3)
RETURNING *;

-- name: ByExternals :many
SELECT id FROM accounts WHERE external_id = ANY(sqlc.arg(ids)::uuid[]);
