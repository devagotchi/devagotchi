-- Sign-in providers set from the hub's settings page (increment 126): SQLite's half of
-- ../postgres/0022_sign_in_settings.sql. Same table and columns. Add-only.
CREATE TABLE sign_in_settings (
  name TEXT PRIMARY KEY,
  kind TEXT NOT NULL CHECK (kind IN ('google', 'github')),
  client_id TEXT NOT NULL,
  sealed_secret TEXT NOT NULL,
  enabled INTEGER NOT NULL,
  updated_at BIGINT NOT NULL,
  updated_by TEXT NOT NULL
);
