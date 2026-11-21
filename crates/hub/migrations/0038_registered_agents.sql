-- Credential-bound agents within an existing account/environment. Identity owns these
-- rows; the AgentHub policy catalog is validated by its API in the same transaction.
CREATE TABLE devagotchi.auth_agents (
  tenant_id TEXT NOT NULL,
  account TEXT NOT NULL,
  id TEXT NOT NULL,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,
  roles TEXT NOT NULL,
  enabled INTEGER NOT NULL CHECK (enabled IN (0,1)),
  wip_limit BIGINT CHECK (wip_limit >= 0),
  version BIGINT NOT NULL CHECK (version > 0),
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  last_seen BIGINT,
  PRIMARY KEY (tenant_id,account,id),
  FOREIGN KEY (tenant_id,account) REFERENCES devagotchi.auth_accounts (tenant_id,id)
);
ALTER TABLE devagotchi.auth_api_keys ADD COLUMN agent_id TEXT;
ALTER TABLE devagotchi.auth_api_keys ADD CONSTRAINT auth_api_keys_agent
  FOREIGN KEY (tenant_id,account,agent_id) REFERENCES devagotchi.auth_agents (tenant_id,account,id);
CREATE TABLE devagotchi.auth_agent_oauth_bindings (
  tenant_id TEXT NOT NULL,
  account TEXT NOT NULL,
  client TEXT NOT NULL REFERENCES devagotchi.oauth_clients (id),
  agent_id TEXT NOT NULL,
  enabled INTEGER NOT NULL CHECK (enabled IN (0,1)),
  version BIGINT NOT NULL CHECK (version > 0),
  PRIMARY KEY (tenant_id,account,client),
  FOREIGN KEY (tenant_id,account,agent_id) REFERENCES devagotchi.auth_agents (tenant_id,account,id)
);
DO $$
DECLARE tab TEXT;
BEGIN
  FOREACH tab IN ARRAY ARRAY['auth_agents','auth_agent_oauth_bindings'] LOOP
    EXECUTE format('ALTER TABLE devagotchi.%I ENABLE ROW LEVEL SECURITY', tab);
    EXECUTE format('ALTER TABLE devagotchi.%I FORCE ROW LEVEL SECURITY', tab);
    EXECUTE format('CREATE POLICY tenant_only ON devagotchi.%I
      USING (tenant_id=current_setting(''devagotchi.tenant'',true))
      WITH CHECK (tenant_id=current_setting(''devagotchi.tenant'',true))', tab);
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname='devagotchi_hub') THEN
      EXECUTE format('GRANT SELECT, INSERT, UPDATE, DELETE ON devagotchi.%I TO devagotchi_hub', tab);
    END IF;
  END LOOP;
END $$;
