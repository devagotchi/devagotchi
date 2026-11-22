-- Durable machine-access coordination. Broker journals remain execution authority.
-- All times are database-clock Unix milliseconds. Snapshot JSON is immutable history;
-- indexed columns provide current state and versioned lifecycle queries.
CREATE TABLE devagotchi.machine_access_settings (
  tenant_id TEXT PRIMARY KEY REFERENCES devagotchi.tenancy_tenants(id),
  version BIGINT NOT NULL CHECK(version>0), value JSONB NOT NULL
);
CREATE TABLE devagotchi.machine_sandbox_policies (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants(id),
  scope TEXT NOT NULL, machine TEXT, generation TEXT,
  allowed BOOLEAN NOT NULL, acknowledged JSONB NOT NULL,
  enabled_by TEXT NOT NULL, enabled_at BIGINT NOT NULL, expires_at BIGINT,
  version BIGINT NOT NULL CHECK(version>0),
  PRIMARY KEY(tenant_id,scope),
  CHECK ((scope='environment' AND machine IS NULL AND generation IS NULL)
    OR (scope<>'environment' AND machine IS NOT NULL AND generation IS NOT NULL)),
  FOREIGN KEY (tenant_id,enabled_by) REFERENCES devagotchi.auth_accounts(tenant_id,id)
);
CREATE TABLE devagotchi.machine_access_requests (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants(id), id TEXT NOT NULL,
  group_id TEXT NOT NULL, account TEXT NOT NULL, principal TEXT NOT NULL,
  machine TEXT NOT NULL, machine_binding JSONB NOT NULL,
  task_binding JSONB, guide_binding JSONB,
  capability TEXT NOT NULL, prepared_item JSONB NOT NULL,
  preset_id TEXT, preset_snapshot JSONB, duration TEXT NOT NULL,
  uses_max BIGINT CHECK(uses_max>0), reason TEXT NOT NULL CHECK(length(reason) BETWEEN 20 AND 8192),
  collapsed BOOLEAN NOT NULL DEFAULT false,
  status TEXT NOT NULL CHECK(status IN ('pending','approved','narrowed','denied','cancelled','expired')),
  created_at BIGINT NOT NULL, expires_at BIGINT NOT NULL CHECK(expires_at>created_at),
  version BIGINT NOT NULL CHECK(version>0),
  decided_by TEXT, decided_at BIGINT, decision_note TEXT, grant_id TEXT,
  PRIMARY KEY(tenant_id,id),
  FOREIGN KEY(tenant_id,account) REFERENCES devagotchi.auth_accounts(tenant_id,id),
  FOREIGN KEY(tenant_id,decided_by) REFERENCES devagotchi.auth_accounts(tenant_id,id),
  CHECK (task_binding IS NULL OR guide_binding IS NULL),
  CHECK (status NOT IN ('approved','narrowed') OR grant_id IS NOT NULL)
);
CREATE INDEX machine_requests_pending ON devagotchi.machine_access_requests(tenant_id,status,created_at,id);
CREATE INDEX machine_requests_group ON devagotchi.machine_access_requests(tenant_id,group_id,id);
CREATE INDEX machine_requests_principal ON devagotchi.machine_access_requests(tenant_id,principal,machine,created_at);
CREATE TABLE devagotchi.machine_grants (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants(id), id TEXT NOT NULL,
  account TEXT NOT NULL, principal TEXT NOT NULL, machine TEXT NOT NULL,
  machine_binding JSONB NOT NULL, task_binding JSONB, guide_binding JSONB,
  request_id TEXT NOT NULL, capability TEXT NOT NULL, item JSONB NOT NULL,
  duration TEXT NOT NULL, uses_max BIGINT CHECK(uses_max>0), uses BIGINT NOT NULL DEFAULT 0 CHECK(uses>=0),
  preset_id TEXT, full_access BOOLEAN NOT NULL, risk_snapshot JSONB NOT NULL,
  approved_by TEXT NOT NULL, approved_at BIGINT NOT NULL, expires_at BIGINT,
  paused BOOLEAN NOT NULL DEFAULT false, ended_at BIGINT, end_reason TEXT, ended_by TEXT,
  version BIGINT NOT NULL CHECK(version>0),
  PRIMARY KEY(tenant_id,id), UNIQUE(tenant_id,request_id),
  FOREIGN KEY(tenant_id,account) REFERENCES devagotchi.auth_accounts(tenant_id,id),
  FOREIGN KEY(tenant_id,approved_by) REFERENCES devagotchi.auth_accounts(tenant_id,id),
  FOREIGN KEY(tenant_id,request_id) REFERENCES devagotchi.machine_access_requests(tenant_id,id),
  CHECK (task_binding IS NULL OR guide_binding IS NULL),
  CHECK (uses_max IS NULL OR uses<=uses_max),
  CHECK ((ended_at IS NULL)=(end_reason IS NULL))
);
ALTER TABLE devagotchi.machine_access_requests ADD CONSTRAINT machine_request_grant
  FOREIGN KEY(tenant_id,grant_id) REFERENCES devagotchi.machine_grants(tenant_id,id);
CREATE INDEX machine_grants_live ON devagotchi.machine_grants(tenant_id,principal,machine,capability) WHERE ended_at IS NULL;
CREATE TABLE devagotchi.machine_admissions (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants(id), id TEXT NOT NULL,
  account TEXT NOT NULL, principal TEXT NOT NULL, machine TEXT NOT NULL, generation TEXT NOT NULL,
  operation_key TEXT NOT NULL, input_sha256 TEXT NOT NULL CHECK(length(input_sha256)=64),
  grant_id TEXT NOT NULL, grant_version BIGINT NOT NULL CHECK(grant_version>0),
  effective JSONB NOT NULL, state TEXT NOT NULL CHECK(state IN ('admitted','finished','retired')),
  admitted_at BIGINT NOT NULL, authority_expires_at BIGINT NOT NULL,
  operation_id TEXT, outcome JSONB, finished_at BIGINT,
  PRIMARY KEY(tenant_id,id), UNIQUE(tenant_id,machine,generation,operation_key),
  FOREIGN KEY(tenant_id,account) REFERENCES devagotchi.auth_accounts(tenant_id,id),
  FOREIGN KEY(tenant_id,grant_id) REFERENCES devagotchi.machine_grants(tenant_id,id)
);
CREATE INDEX machine_admissions_grant ON devagotchi.machine_admissions(tenant_id,grant_id,state);
CREATE TABLE devagotchi.machine_access_notifications (
  tenant_id TEXT NOT NULL, grant_id TEXT NOT NULL, version BIGINT NOT NULL CHECK(version>0),
  machine TEXT NOT NULL, generation TEXT NOT NULL, reason TEXT NOT NULL,
  created_at BIGINT NOT NULL, delivered_at BIGINT, attempts BIGINT NOT NULL DEFAULT 0 CHECK(attempts>=0),
  next_attempt_at BIGINT NOT NULL, last_error TEXT,
  PRIMARY KEY(tenant_id,grant_id,version),
  FOREIGN KEY(tenant_id,grant_id) REFERENCES devagotchi.machine_grants(tenant_id,id)
);
CREATE INDEX machine_access_notifications_due ON devagotchi.machine_access_notifications(tenant_id,next_attempt_at) WHERE delivered_at IS NULL;
CREATE TABLE devagotchi.machine_resume_alerts (
  tenant_id TEXT NOT NULL, id TEXT NOT NULL, grant_id TEXT NOT NULL,
  person TEXT NOT NULL, session_id TEXT NOT NULL, reason TEXT NOT NULL,
  shown_at BIGINT NOT NULL, acknowledged_at BIGINT,
  action TEXT CHECK(action IN ('continue','revoke','pause')), version BIGINT NOT NULL CHECK(version>0),
  PRIMARY KEY(tenant_id,id),
  FOREIGN KEY(tenant_id,grant_id) REFERENCES devagotchi.machine_grants(tenant_id,id),
  FOREIGN KEY(tenant_id,person) REFERENCES devagotchi.auth_accounts(tenant_id,id),
  CHECK((acknowledged_at IS NULL)=(action IS NULL))
);
CREATE UNIQUE INDEX machine_resume_alerts_open ON devagotchi.machine_resume_alerts(tenant_id,grant_id,person) WHERE acknowledged_at IS NULL;
DO $$
DECLARE tab TEXT;
BEGIN
  FOREACH tab IN ARRAY ARRAY['machine_access_settings','machine_sandbox_policies','machine_access_requests',
    'machine_grants','machine_admissions','machine_access_notifications','machine_resume_alerts'] LOOP
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
