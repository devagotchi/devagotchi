-- API keys (increment 125, accounts/api_keys.rs): SQLite's half of
-- ../postgres/0021_api_keys.sql. Same table and columns; no row-level security and no function
-- here. Add-only.
CREATE TABLE api_keys (
  tenant_id TEXT NOT NULL REFERENCES tenancy_tenants (id),
  id TEXT NOT NULL,
  account TEXT NOT NULL,
  name TEXT NOT NULL,
  hash TEXT NOT NULL UNIQUE,
  hint TEXT NOT NULL,
  scopes TEXT NOT NULL,
  projects TEXT NOT NULL,
  machines TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  expires_at BIGINT NOT NULL,
  last_used_at BIGINT,
  revoked_at BIGINT,
  PRIMARY KEY (tenant_id, id)
);
CREATE INDEX api_keys_by_account ON api_keys (tenant_id, account);
