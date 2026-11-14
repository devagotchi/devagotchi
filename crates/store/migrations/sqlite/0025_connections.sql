-- Connections (increment 140, accounts/connections.rs): SQLite's half of
-- ../postgres/0025_connections.sql. Same table and columns; no row-level security here.
-- Add-only.
CREATE TABLE connections (
  tenant_id TEXT NOT NULL REFERENCES tenancy_tenants (id),
  name TEXT NOT NULL,
  service TEXT NOT NULL,
  sealed_key TEXT NOT NULL,
  hint TEXT NOT NULL,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  last_used_at BIGINT,
  PRIMARY KEY (tenant_id, name)
);
