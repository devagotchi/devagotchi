-- Signal sinks set on /admin (hub-settings M5, notify/signals/stored.rs): one row per sink,
-- beside the ones the config file names (those are not here). The bearer token and the signing
-- secret are sealed under the key in the state directory, so this table alone gives nothing
-- away. Where each sink is up to stays in signal_cursors, by name. Hub bookkeeping: no
-- tenant's data. Add-only.
CREATE TABLE devagotchi.signal_sinks (
  name TEXT PRIMARY KEY CHECK (name ~ '^[a-z0-9][a-z0-9-]{0,62}$'),
  url TEXT NOT NULL,
  sealed_token TEXT NOT NULL,
  sealed_secret TEXT NOT NULL,
  fields TEXT NOT NULL CHECK (fields IN ('metadata', 'full')),
  sources TEXT NOT NULL,
  enabled INTEGER NOT NULL,
  updated_at BIGINT NOT NULL,
  updated_by TEXT NOT NULL
);

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub') THEN
    GRANT SELECT, INSERT, UPDATE, DELETE ON devagotchi.signal_sinks TO devagotchi_hub;
  END IF;
END $$;
