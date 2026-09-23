-- name: GetPerson :one
SELECT * FROM people WHERE id = $1;

-- name: ListByIds :many
SELECT id, name FROM people WHERE id = ANY(sqlc.slice('ids')::bigint[]);

-- name: Search :many
SELECT id, name, feeling FROM people
WHERE name ILIKE sqlc.arg(pattern) AND (sqlc.narg(min_age)::int IS NULL OR age >= sqlc.narg(min_age));

-- name: PeopleWithPets :many
SELECT sqlc.embed(people), sqlc.embed(pets) FROM people JOIN pets ON pets.owner_id = people.id;

-- name: CreatePerson :one
INSERT INTO people (name, tags, feeling) VALUES ($1, $2, $3) RETURNING id, created;

-- name: Rename :execrows
UPDATE people SET name = @new_name WHERE id = @id;

-- name: Touch :exec
UPDATE people SET created = now();

-- name: Remove :execresult
DELETE FROM people WHERE id = $1;

-- name: CopyPets :copyfrom
INSERT INTO pets (owner_id, name) VALUES ($1, $2);

-- name: BatchNames :batchmany
SELECT name FROM people WHERE age = $1;

-- name: BatchOne :batchone
SELECT id FROM people WHERE name = $1;

-- name: BatchDel :batchexec
DELETE FROM pets WHERE id = $1;

-- name: CountPeople :one
SELECT count(*) AS total, max(created) FROM people;

-- name: Scalar :one
SELECT name FROM people WHERE id = $1;
