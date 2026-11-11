-- Sign-in providers set from the hub's settings page (increment 126,
-- accounts/sign_in_settings.rs): the hub's own configuration, one row per provider. The client
-- secret is sealed under a key in the state directory (sealed.rs), so this table alone gives
-- nothing away. Hub bookkeeping, like signal_cursors: no tenant's data. Add-only. Kept in step
-- with ../sqlite/0022_sign_in_settings.sql.
CREATE TABLE devagotchi.sign_in_settings (
  name TEXT PRIMARY KEY,
  kind TEXT NOT NULL CHECK (kind IN ('google', 'github')),
  client_id TEXT NOT NULL,
  sealed_secret TEXT NOT NULL,
  enabled INTEGER NOT NULL,
  updated_at BIGINT NOT NULL,
  updated_by TEXT NOT NULL
);

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub') THEN
    GRANT SELECT, INSERT, UPDATE, DELETE ON devagotchi.sign_in_settings TO devagotchi_hub;
  END IF;
END $$;
