-- Connections (increment 140, accounts/connections.rs): the outside services a tenant's agents
-- and apps call through the hub (Resend, Stripe, GitHub …). The key is sealed under a key in
-- the state directory (sealed.rs) and never shown again; `hint` is its last four characters.
-- Under the tenant's row-level security. Add-only. Kept in step with
-- ../sqlite/0025_connections.sql.
CREATE TABLE devagotchi.connections (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants (id),
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

ALTER TABLE devagotchi.connections ENABLE ROW LEVEL SECURITY;
ALTER TABLE devagotchi.connections FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_only ON devagotchi.connections
  USING (tenant_id = current_setting('devagotchi.tenant', true))
  WITH CHECK (tenant_id = current_setting('devagotchi.tenant', true));

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub') THEN
    GRANT SELECT, INSERT, UPDATE, DELETE ON devagotchi.connections TO devagotchi_hub;
  END IF;
END $$;
