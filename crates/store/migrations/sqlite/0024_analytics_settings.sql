-- AgentHub's statistics export set from the hub's settings page (increment 138): SQLite's
-- half of ../postgres/0024_analytics_settings.sql. Same table and columns. Add-only.
CREATE TABLE analytics_settings (
  name TEXT PRIMARY KEY CHECK (name IN ('clickhouse')),
  url TEXT NOT NULL,
  database_name TEXT NOT NULL,
  user_name TEXT NOT NULL,
  sealed_password TEXT NOT NULL,
  enabled INTEGER NOT NULL,
  updated_at BIGINT NOT NULL,
  updated_by TEXT NOT NULL
);
