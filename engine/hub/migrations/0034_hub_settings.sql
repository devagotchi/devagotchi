-- The settings the owner changes on /admin while the hub runs (config/live.rs,
-- ops/hub_settings.rs): one row per setting, its value as the page shows it. A value the config
-- file or its environment variable sets wins over the row, which then waits unused. Hub
-- bookkeeping, like sign_in_settings and analytics_settings: no tenant's data. Add-only.
CREATE TABLE devagotchi.hub_settings (
  key TEXT PRIMARY KEY CHECK (length(key) BETWEEN 1 AND 128),
  value TEXT NOT NULL CHECK (length(value) <= 1024),
  updated_at BIGINT NOT NULL,
  updated_by TEXT NOT NULL
);

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub') THEN
    GRANT SELECT, INSERT, UPDATE, DELETE ON devagotchi.hub_settings TO devagotchi_hub;
  END IF;
END $$;
