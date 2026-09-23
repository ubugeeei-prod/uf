CREATE TABLE accounts (
  id bigserial PRIMARY KEY,
  external_id uuid NOT NULL,
  settings jsonb NOT NULL DEFAULT '{}',
  created_at timestamptz NOT NULL DEFAULT now(),
  spotify_url text,
  balance numeric(12, 2) NOT NULL DEFAULT 0
);
