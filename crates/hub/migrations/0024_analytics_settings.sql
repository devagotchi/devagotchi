-- AgentHub's statistics export set from the hub's settings page (increment 138,
-- accounts/analytics_settings.rs): which ClickHouse the board events go to. One row per kind
-- of export, and ClickHouse is the only kind. The password is sealed under a key in the state
-- directory (sealed.rs), so this table alone gives nothing away. Hub bookkeeping, like
-- sign_in_settings: no tenant's data. Add-only. Kept in step with
-- ../sqlite/0024_analytics_settings.sql.
CREATE TABLE devagotchi.analytics_settings (
  name TEXT PRIMARY KEY CHECK (name IN ('clickhouse')),
  url TEXT NOT NULL,
  database_name TEXT NOT NULL,
  user_name TEXT NOT NULL,
  sealed_password TEXT NOT NULL,
  enabled INTEGER NOT NULL,
  updated_at BIGINT NOT NULL,
  updated_by TEXT NOT NULL
);

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub') THEN
    GRANT SELECT, INSERT, UPDATE, DELETE ON devagotchi.analytics_settings TO devagotchi_hub;
  END IF;
END $$;
