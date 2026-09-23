-- name: GetPerson :one
SELECT * FROM people WHERE id = ?;
-- name: ByIds :many
SELECT id, name FROM people WHERE id IN (sqlc.slice('ids')) AND name <> @skip;
-- name: Embed :many
SELECT sqlc.embed(people), sqlc.embed(pets) FROM people JOIN pets ON pets.owner_id = people.id;
-- name: Create :execlastid
INSERT INTO people (name) VALUES (?);
-- name: Nar :many
SELECT id FROM people WHERE age > sqlc.narg('min') LIMIT ?;
-- name: Num :one
SELECT id FROM people WHERE name = ?2 AND age = ?1;
-- name: Count :one
SELECT count(*) AS n, max(age) AS oldest, 1 AS one FROM people;
-- name: Ret :one
INSERT INTO pets (owner_id, name) VALUES (?, ?) RETURNING *;
-- name: Mixed :many
SELECT id FROM people WHERE id IN (sqlc.slice('ids')) AND name = ? AND age IN (sqlc.slice(ages)) AND big = @big;
-- name: Either :many
SELECT id FROM people WHERE name = @n OR vc = @n;
-- name: Rename :execrows
UPDATE people SET name = @new_name WHERE id = @id;
-- name: Remove :execresult
DELETE FROM people WHERE id = ?;
-- name: Touch :exec
UPDATE people SET ratio = ratio + 1;
-- name: CopyPets :copyfrom
INSERT INTO pets (owner_id, name) VALUES (?, ?);
-- name: PetNames :batchmany
SELECT name FROM pets WHERE owner_id = ?;
