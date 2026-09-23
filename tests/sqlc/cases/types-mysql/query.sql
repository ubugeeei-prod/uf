-- name: GetPerson :one
SELECT * FROM people WHERE id = ?;
-- name: ByIds :many
SELECT id, name FROM people WHERE id IN (sqlc.slice('ids')) AND name <> sqlc.arg(skip);
-- name: Embed :many
SELECT sqlc.embed(people), sqlc.embed(pets) FROM people JOIN pets ON pets.owner_id = people.id;
-- name: Create :execlastid
INSERT INTO people (name, ratio, mood) VALUES (?, ?, ?);
-- name: Nar :many
SELECT id FROM people WHERE age > sqlc.narg('min') LIMIT ?;
-- name: CopyPets :copyfrom
INSERT INTO pets (owner_id, name) VALUES (?, ?);
-- name: Touch :exec
UPDATE people SET ratio = ratio + 1 WHERE id = ?;
-- name: Rename :execrows
UPDATE people SET name = sqlc.arg(new_name) WHERE id = sqlc.arg(id);
-- name: Remove :execresult
DELETE FROM people WHERE id = ?;
-- name: Either :many
SELECT id FROM people WHERE name = sqlc.arg(n) OR tiny = sqlc.arg(n);
