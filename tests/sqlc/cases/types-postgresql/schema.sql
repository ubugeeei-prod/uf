CREATE TYPE mood AS ENUM ('sad', 'ok', 'happy');
CREATE TABLE people (
  id bigserial PRIMARY KEY,
  name text NOT NULL,
  nick varchar(20),
  age int4,
  small int2 NOT NULL DEFAULT 0,
  score numeric(10,2),
  ratio float8 NOT NULL DEFAULT 0,
  active boolean NOT NULL DEFAULT true,
  born date,
  wake time,
  created timestamptz NOT NULL DEFAULT now(),
  local_at timestamp,
  data jsonb NOT NULL DEFAULT '{}',
  raw bytea,
  tags text[] NOT NULL DEFAULT '{}',
  grid int4[][],
  feeling mood NOT NULL DEFAULT 'ok',
  feelings mood[],
  uid uuid,
  addr inet
);
CREATE TABLE pets (
  id serial PRIMARY KEY,
  owner_id bigint NOT NULL REFERENCES people(id),
  name text NOT NULL
);
