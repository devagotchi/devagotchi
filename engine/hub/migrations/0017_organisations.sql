-- Organisations, their members and teams, and who works which project (increment 114,
-- orgs.rs). An organisation is a tenant of its own (`tenancy_tenants.kind`
-- 'organisation'): its projects are hosted boards owned by `org-<tenant id>`, so every row
-- of them stays under the tenant's row-level security, and an account of another tenant
-- reaches them only through the one resolver (`board::mcp::Hub::board_access`), which
-- scopes each transaction to the organisation's tenant.
--
-- Roles, highest first: owner, admin, member, agent, viewer. An organisation's role
-- applies to each of its projects whose visibility is 'org'; a project's own members
-- (a subject, or a team of the organisation) hold the role their row names on that
-- project, and the higher of the two counts. Admins and owners reach every project.
-- Invites keep a SHA-256 of their one-time token, never the token. Add-only. Kept in step
-- with ../sqlite/0017_organisations.sql.

ALTER TABLE devagotchi.tenancy_tenants ADD COLUMN name TEXT;
ALTER TABLE devagotchi.tenancy_tenants ADD COLUMN kind TEXT NOT NULL DEFAULT 'account'
  CHECK (kind IN ('account', 'organisation'));

CREATE TABLE devagotchi.org_members (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants (id),
  subject TEXT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member', 'agent', 'viewer')),
  added_by TEXT NOT NULL,
  added_at BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, subject)
);
CREATE INDEX org_members_by_subject ON devagotchi.org_members (subject);

CREATE TABLE devagotchi.org_teams (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants (id),
  team TEXT NOT NULL,
  name TEXT NOT NULL,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, team)
);

CREATE TABLE devagotchi.org_team_members (
  tenant_id TEXT NOT NULL,
  team TEXT NOT NULL,
  subject TEXT NOT NULL,
  added_by TEXT NOT NULL,
  added_at BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, team, subject),
  FOREIGN KEY (tenant_id, team) REFERENCES devagotchi.org_teams (tenant_id, team) ON DELETE CASCADE
);
CREATE INDEX org_team_members_by_subject ON devagotchi.org_team_members (subject);

CREATE TABLE devagotchi.project_access (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants (id),
  owner_id TEXT NOT NULL,
  project TEXT NOT NULL,
  visibility TEXT NOT NULL DEFAULT 'org' CHECK (visibility IN ('org', 'members')),
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, owner_id, project)
);

CREATE TABLE devagotchi.project_members (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants (id),
  owner_id TEXT NOT NULL,
  project TEXT NOT NULL,
  principal_kind TEXT NOT NULL CHECK (principal_kind IN ('subject', 'team')),
  principal TEXT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('admin', 'member', 'agent', 'viewer')),
  added_by TEXT NOT NULL,
  added_at BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, owner_id, project, principal_kind, principal)
);
CREATE INDEX project_members_by_principal ON devagotchi.project_members (principal_kind, principal);

CREATE TABLE devagotchi.org_invites (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants (id),
  hash TEXT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member', 'agent', 'viewer')),
  note TEXT,
  invited_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  expires_at BIGINT NOT NULL,
  accepted_by TEXT,
  accepted_at BIGINT,
  revoked_at BIGINT,
  PRIMARY KEY (tenant_id, hash),
  CONSTRAINT org_invites_hash UNIQUE (hash)
);

DO $$
DECLARE name text;
BEGIN
  FOREACH name IN ARRAY ARRAY[
    'org_members', 'org_teams', 'org_team_members', 'project_access', 'project_members',
    'org_invites'
  ] LOOP
    EXECUTE format('ALTER TABLE devagotchi.%I ENABLE ROW LEVEL SECURITY', name);
    EXECUTE format('ALTER TABLE devagotchi.%I FORCE ROW LEVEL SECURITY', name);
    EXECUTE format(
      'CREATE POLICY tenant_only ON devagotchi.%I
       USING (tenant_id = current_setting(''devagotchi.tenant'', true))
       WITH CHECK (tenant_id = current_setting(''devagotchi.tenant'', true))', name);
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub') THEN
      EXECUTE format('GRANT SELECT, INSERT, UPDATE, DELETE ON devagotchi.%I TO devagotchi_hub', name);
    END IF;
  END LOOP;
END $$;

-- What one subject belongs to, across tenants, and nothing else: the organisations it is a
-- member of with its role there, and the projects it was added to by name or through a
-- team, with that role. Each organisation's details are then read in a transaction scoped
-- to it, as its own requests are.
CREATE FUNCTION devagotchi.orgs_of(who text)
RETURNS TABLE (tenant_id text, role text)
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = devagotchi, pg_temp AS $$
  SELECT m.tenant_id, m.role FROM org_members m WHERE m.subject = who
  ORDER BY m.tenant_id
$$;

CREATE FUNCTION devagotchi.projects_of(who text)
RETURNS TABLE (tenant_id text, owner_id text, project text, role text)
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = devagotchi, pg_temp AS $$
  SELECT p.tenant_id, p.owner_id, p.project, p.role FROM project_members p
   WHERE p.principal_kind = 'subject' AND p.principal = who
  UNION ALL
  SELECT p.tenant_id, p.owner_id, p.project, p.role FROM project_members p
    JOIN org_team_members t
      ON t.tenant_id = p.tenant_id AND t.team = p.principal AND t.subject = who
   WHERE p.principal_kind = 'team'
  ORDER BY 1, 2, 3
$$;

-- The tenant an invite belongs to, by the SHA-256 of its token: what lets the person who
-- holds the token accept it before they belong to that tenant.
CREATE FUNCTION devagotchi.invite_tenant(token_hash text)
RETURNS text LANGUAGE sql STABLE SECURITY DEFINER SET search_path = devagotchi, pg_temp AS $$
  SELECT tenant_id FROM org_invites WHERE hash = token_hash
$$;

REVOKE ALL ON FUNCTION devagotchi.orgs_of(text) FROM PUBLIC;
REVOKE ALL ON FUNCTION devagotchi.projects_of(text) FROM PUBLIC;
REVOKE ALL ON FUNCTION devagotchi.invite_tenant(text) FROM PUBLIC;
DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub') THEN
    GRANT EXECUTE ON FUNCTION devagotchi.orgs_of(text) TO devagotchi_hub;
    GRANT EXECUTE ON FUNCTION devagotchi.projects_of(text) TO devagotchi_hub;
    GRANT EXECUTE ON FUNCTION devagotchi.invite_tenant(text) TO devagotchi_hub;
  END IF;
END $$;
