-- API keys (increment 125, accounts/api_keys.rs): what an account hands an agent that cannot
-- do the OAuth sign-in, limited to the scopes, projects and machines the person chose and to a
-- duration. Keeps a SHA-256 of the key, never the key. Under the tenant's row-level security.
-- Add-only. Kept in step with ../sqlite/0021_api_keys.sql.
CREATE TABLE devagotchi.api_keys (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants (id),
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
CREATE INDEX api_keys_by_account ON devagotchi.api_keys (tenant_id, account);

ALTER TABLE devagotchi.api_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE devagotchi.api_keys FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_only ON devagotchi.api_keys
  USING (tenant_id = current_setting('devagotchi.tenant', true))
  WITH CHECK (tenant_id = current_setting('devagotchi.tenant', true));

-- The tenant a key belongs to, by its hash, and nothing else: what lets the hub scope the
-- transaction that reads the key.
CREATE FUNCTION devagotchi.api_key_tenant(key_hash text)
RETURNS text LANGUAGE sql STABLE SECURITY DEFINER SET search_path = devagotchi, pg_temp AS $$
  SELECT tenant_id FROM api_keys WHERE hash = key_hash
$$;
REVOKE ALL ON FUNCTION devagotchi.api_key_tenant(text) FROM PUBLIC;

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub') THEN
    GRANT SELECT, INSERT, UPDATE, DELETE ON devagotchi.api_keys TO devagotchi_hub;
    GRANT EXECUTE ON FUNCTION devagotchi.api_key_tenant(text) TO devagotchi_hub;
  END IF;
END $$;
