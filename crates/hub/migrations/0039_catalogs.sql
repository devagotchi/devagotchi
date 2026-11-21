-- Environment-scoped configuration data; grants remain a separate authority owner.
CREATE TABLE devagotchi.catalog_capabilities (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants(id), id TEXT NOT NULL,
  version BIGINT NOT NULL CHECK(version>0), label TEXT NOT NULL, description TEXT NOT NULL,
  selector_schema JSONB NOT NULL, sandbox_levels JSONB NOT NULL, default_duration TEXT NOT NULL,
  PRIMARY KEY(tenant_id,id)
);
CREATE TABLE devagotchi.catalog_risks (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants(id), id TEXT NOT NULL,
  version BIGINT NOT NULL CHECK(version>0), title TEXT NOT NULL, plain_text TEXT NOT NULL,
  severity TEXT NOT NULL CHECK(severity IN ('low','med','high')), mitigation TEXT NOT NULL, source_url TEXT,
  PRIMARY KEY(tenant_id,id)
);
CREATE TABLE devagotchi.catalog_capability_risks (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants(id), id TEXT NOT NULL,
  version BIGINT NOT NULL CHECK(version>0), capability_id TEXT NOT NULL, risk_id TEXT NOT NULL, when_selector JSONB NOT NULL,
  PRIMARY KEY(tenant_id,id),
  FOREIGN KEY(tenant_id,capability_id) REFERENCES devagotchi.catalog_capabilities(tenant_id,id),
  FOREIGN KEY(tenant_id,risk_id) REFERENCES devagotchi.catalog_risks(tenant_id,id)
);
CREATE TABLE devagotchi.catalog_shell_profiles (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants(id), id TEXT NOT NULL,
  version BIGINT NOT NULL CHECK(version>0), os TEXT NOT NULL, label TEXT NOT NULL,
  argv_prefix JSONB NOT NULL, env_allow JSONB NOT NULL, cwd_default TEXT NOT NULL, pty BOOLEAN NOT NULL,
  PRIMARY KEY(tenant_id,id)
);
CREATE TABLE devagotchi.catalog_grant_presets (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants(id), id TEXT NOT NULL,
  version BIGINT NOT NULL CHECK(version>0), label TEXT NOT NULL, description TEXT NOT NULL,
  items JSONB NOT NULL, duration TEXT NOT NULL, full_access BOOLEAN NOT NULL, requires_policy TEXT,
  is_default BOOLEAN NOT NULL DEFAULT false,
  CHECK(NOT full_access OR requires_policy IS NOT DISTINCT FROM 'unsandboxed'),
  PRIMARY KEY(tenant_id,id)
);
CREATE UNIQUE INDEX catalog_one_default_preset ON devagotchi.catalog_grant_presets(tenant_id) WHERE is_default;
CREATE TABLE devagotchi.catalog_docker_runtime_presets (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants(id), id TEXT NOT NULL,
  version BIGINT NOT NULL CHECK(version>0), os TEXT NOT NULL, label TEXT NOT NULL, what TEXT NOT NULL,
  facts JSONB NOT NULL, operator_commands JSONB NOT NULL, agent_argv JSONB NOT NULL, verify_argv JSONB NOT NULL,
  hosts JSONB NOT NULL, risk_ids JSONB NOT NULL, licence_confirm BOOLEAN NOT NULL,
  PRIMARY KEY(tenant_id,id)
);
CREATE TABLE devagotchi.catalog_router_endpoints (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants(id), id TEXT NOT NULL,
  version BIGINT NOT NULL CHECK(version>0), label TEXT NOT NULL, url TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('litellm','openrouter','direct')),
  PRIMARY KEY(tenant_id,id)
);
CREATE TABLE devagotchi.catalog_chat_apps (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants(id), id TEXT NOT NULL,
  version BIGINT NOT NULL CHECK(version>0), label TEXT NOT NULL, provider_ids JSONB NOT NULL, connect_template TEXT NOT NULL,
  PRIMARY KEY(tenant_id,id)
);
CREATE TABLE devagotchi.catalog_seed_marks (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants(id), name TEXT NOT NULL,
  PRIMARY KEY(tenant_id,name)
);
DO $$
DECLARE tab TEXT;
BEGIN
  FOREACH tab IN ARRAY ARRAY['catalog_capabilities','catalog_risks','catalog_capability_risks',
    'catalog_shell_profiles','catalog_grant_presets','catalog_docker_runtime_presets',
    'catalog_router_endpoints','catalog_chat_apps','catalog_seed_marks'] LOOP
    EXECUTE format('ALTER TABLE devagotchi.%I ENABLE ROW LEVEL SECURITY',tab);
    EXECUTE format('ALTER TABLE devagotchi.%I FORCE ROW LEVEL SECURITY',tab);
    EXECUTE format('CREATE POLICY tenant_only ON devagotchi.%I
      USING (tenant_id=current_setting(''devagotchi.tenant'',true))
      WITH CHECK (tenant_id=current_setting(''devagotchi.tenant'',true))',tab);
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname='devagotchi_hub') THEN
      EXECUTE format('GRANT SELECT,INSERT,UPDATE,DELETE ON devagotchi.%I TO devagotchi_hub',tab);
    END IF;
  END LOOP;
END $$;
