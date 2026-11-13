-- Sign in with Shippin (Nomen) and any OpenID Connect provider (increment 134): SQLite's half of
-- ../postgres/0023_sign_in_oidc.sql. SQLite cannot widen a CHECK in place, so the table is made
-- again with the same rows; same columns as Postgres.
CREATE TABLE sign_in_settings_next (
  name TEXT PRIMARY KEY,
  kind TEXT NOT NULL CHECK (kind IN ('google', 'github', 'oidc')),
  client_id TEXT NOT NULL,
  sealed_secret TEXT NOT NULL,
  enabled INTEGER NOT NULL,
  updated_at BIGINT NOT NULL,
  updated_by TEXT NOT NULL,
  issuer TEXT NOT NULL DEFAULT '',
  label TEXT NOT NULL DEFAULT ''
);
INSERT INTO sign_in_settings_next (name, kind, client_id, sealed_secret, enabled, updated_at, updated_by)
  SELECT name, kind, client_id, sealed_secret, enabled, updated_at, updated_by FROM sign_in_settings;
DROP TABLE sign_in_settings;
ALTER TABLE sign_in_settings_next RENAME TO sign_in_settings;
