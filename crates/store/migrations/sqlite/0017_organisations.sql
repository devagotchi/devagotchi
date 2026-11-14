-- Organisations, their members and teams, and who works which project (increment 114,
-- orgs.rs): SQLite's half of ../postgres/0017_organisations.sql. Same tables and columns;
-- no row-level security and no functions here, so orgs.rs answers `orgs_of`,
-- `projects_of` and `invite_tenant` with plain queries on this backend. Add-only.

ALTER TABLE tenancy_tenants ADD COLUMN name TEXT;
ALTER TABLE tenancy_tenants ADD COLUMN kind TEXT NOT NULL DEFAULT 'account'
  CHECK (kind IN ('account', 'organisation'));

CREATE TABLE org_members (
  tenant_id TEXT NOT NULL REFERENCES tenancy_tenants (id),
  subject TEXT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member', 'agent', 'viewer')),
  added_by TEXT NOT NULL,
  added_at BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, subject)
);
CREATE INDEX org_members_by_subject ON org_members (subject);

CREATE TABLE org_teams (
  tenant_id TEXT NOT NULL REFERENCES tenancy_tenants (id),
  team TEXT NOT NULL,
  name TEXT NOT NULL,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, team)
);

CREATE TABLE org_team_members (
  tenant_id TEXT NOT NULL,
  team TEXT NOT NULL,
  subject TEXT NOT NULL,
  added_by TEXT NOT NULL,
  added_at BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, team, subject),
  FOREIGN KEY (tenant_id, team) REFERENCES org_teams (tenant_id, team) ON DELETE CASCADE
);
CREATE INDEX org_team_members_by_subject ON org_team_members (subject);

CREATE TABLE project_access (
  tenant_id TEXT NOT NULL REFERENCES tenancy_tenants (id),
  owner_id TEXT NOT NULL,
  project TEXT NOT NULL,
  visibility TEXT NOT NULL DEFAULT 'org' CHECK (visibility IN ('org', 'members')),
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, owner_id, project)
);

CREATE TABLE project_members (
  tenant_id TEXT NOT NULL REFERENCES tenancy_tenants (id),
  owner_id TEXT NOT NULL,
  project TEXT NOT NULL,
  principal_kind TEXT NOT NULL CHECK (principal_kind IN ('subject', 'team')),
  principal TEXT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('admin', 'member', 'agent', 'viewer')),
  added_by TEXT NOT NULL,
  added_at BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, owner_id, project, principal_kind, principal)
);
CREATE INDEX project_members_by_principal ON project_members (principal_kind, principal);

CREATE TABLE org_invites (
  tenant_id TEXT NOT NULL REFERENCES tenancy_tenants (id),
  hash TEXT NOT NULL UNIQUE,
  role TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member', 'agent', 'viewer')),
  note TEXT,
  invited_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  expires_at BIGINT NOT NULL,
  accepted_by TEXT,
  accepted_at BIGINT,
  revoked_at BIGINT,
  PRIMARY KEY (tenant_id, hash)
);
