-- AgentHub's boards in Postgres, its own migrations with its own history
-- (`_agenthub_migrations`). This is the schema the hub's migrations 0014, 0015, 0016 and
-- 0018 made before AgentHub owned it: on a database they already migrated every statement
-- finds what it would make and leaves it, so the boards there are adopted, not rebuilt.
--
-- A board belongs to a tenant, the scope its host sets (`devagotchi.tenant`, row-level
-- security on every table); AgentHub keeps no table of tenants and refers to none.

CREATE SCHEMA IF NOT EXISTS devagotchi;

CREATE TABLE IF NOT EXISTS devagotchi.board_projects (
  tenant_id TEXT NOT NULL,
  id TEXT NOT NULL,
  source_hash TEXT NOT NULL,
  imported_at BIGINT NOT NULL,
  owner_id TEXT,
  project_name TEXT,
  PRIMARY KEY (tenant_id, id)
);
-- The hub's tenants were the parent of a board before AgentHub stood alone.
ALTER TABLE devagotchi.board_projects DROP CONSTRAINT IF EXISTS board_projects_tenant_id_fkey;
CREATE UNIQUE INDEX IF NOT EXISTS board_projects_owner_name
  ON devagotchi.board_projects (tenant_id, owner_id, project_name);

CREATE TABLE IF NOT EXISTS devagotchi.board_metadata (
  tenant_id TEXT NOT NULL,
  board_id TEXT NOT NULL,
  key TEXT NOT NULL,
  value TEXT NOT NULL,
  PRIMARY KEY (tenant_id, board_id, key),
  FOREIGN KEY (tenant_id, board_id) REFERENCES devagotchi.board_projects (tenant_id, id)
);
CREATE TABLE IF NOT EXISTS devagotchi.board_settings (
  tenant_id TEXT NOT NULL,
  board_id TEXT NOT NULL,
  key TEXT NOT NULL,
  value TEXT NOT NULL,
  PRIMARY KEY (tenant_id, board_id, key),
  FOREIGN KEY (tenant_id, board_id) REFERENCES devagotchi.board_projects (tenant_id, id)
);
CREATE TABLE IF NOT EXISTS devagotchi.board_fence (
  tenant_id TEXT NOT NULL,
  board_id TEXT NOT NULL,
  id BIGINT NOT NULL CHECK (id = 1),
  value BIGINT NOT NULL CHECK (value >= 0),
  PRIMARY KEY (tenant_id, board_id, id),
  FOREIGN KEY (tenant_id, board_id) REFERENCES devagotchi.board_projects (tenant_id, id)
);
CREATE TABLE IF NOT EXISTS devagotchi.board_tickets (
  tenant_id TEXT NOT NULL,
  board_id TEXT NOT NULL,
  id TEXT NOT NULL,
  kind TEXT NOT NULL DEFAULT 'task',
  title TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  state TEXT NOT NULL,
  priority BIGINT NOT NULL DEFAULT 2,
  labels TEXT NOT NULL DEFAULT '[]',
  parent TEXT,
  child_seq BIGINT NOT NULL DEFAULT 0,
  assignee TEXT,
  holder TEXT,
  lease_id TEXT,
  lease_ttl_ms BIGINT,
  lease_expires_at BIGINT,
  heartbeat_at BIGINT,
  fence BIGINT NOT NULL DEFAULT 0,
  version BIGINT NOT NULL DEFAULT 1,
  expiry_count BIGINT NOT NULL DEFAULT 0,
  last_worker TEXT,
  progress TEXT,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  closed_at BIGINT,
  machine TEXT,
  repo TEXT,
  environment TEXT,
  agent TEXT,
  session TEXT,
  fingerprint TEXT,
  occurrences BIGINT NOT NULL DEFAULT 1,
  last_seen_at BIGINT,
  PRIMARY KEY (tenant_id, board_id, id),
  FOREIGN KEY (tenant_id, board_id) REFERENCES devagotchi.board_projects (tenant_id, id),
  FOREIGN KEY (tenant_id, board_id, parent)
    REFERENCES devagotchi.board_tickets (tenant_id, board_id, id)
    DEFERRABLE INITIALLY DEFERRED,
  CHECK ((holder IS NULL) = (lease_id IS NULL))
);
CREATE INDEX IF NOT EXISTS board_tickets_queue ON devagotchi.board_tickets
  (tenant_id, board_id, state, priority, created_at) WHERE holder IS NULL;
CREATE INDEX IF NOT EXISTS board_tickets_lease ON devagotchi.board_tickets
  (tenant_id, board_id, lease_expires_at) WHERE holder IS NOT NULL;
CREATE INDEX IF NOT EXISTS board_tickets_parent
  ON devagotchi.board_tickets (tenant_id, board_id, parent);
CREATE INDEX IF NOT EXISTS board_tickets_fingerprint ON devagotchi.board_tickets
  (tenant_id, board_id, fingerprint);
CREATE TABLE IF NOT EXISTS devagotchi.board_deps (
  tenant_id TEXT NOT NULL,
  board_id TEXT NOT NULL,
  ticket TEXT NOT NULL,
  depends_on TEXT NOT NULL,
  kind TEXT NOT NULL DEFAULT 'blocks',
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, board_id, ticket, depends_on, kind),
  FOREIGN KEY (tenant_id, board_id, ticket)
    REFERENCES devagotchi.board_tickets (tenant_id, board_id, id),
  FOREIGN KEY (tenant_id, board_id, depends_on)
    REFERENCES devagotchi.board_tickets (tenant_id, board_id, id),
  CHECK (ticket <> depends_on)
);
CREATE INDEX IF NOT EXISTS board_deps_reverse
  ON devagotchi.board_deps (tenant_id, board_id, depends_on);
CREATE TABLE IF NOT EXISTS devagotchi.board_notes (
  tenant_id TEXT NOT NULL,
  board_id TEXT NOT NULL,
  seq BIGINT NOT NULL,
  ticket TEXT NOT NULL,
  author TEXT NOT NULL,
  kind TEXT NOT NULL,
  body TEXT NOT NULL,
  to_principal TEXT,
  fence BIGINT,
  created_at BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, board_id, seq),
  FOREIGN KEY (tenant_id, board_id, ticket)
    REFERENCES devagotchi.board_tickets (tenant_id, board_id, id)
);
CREATE INDEX IF NOT EXISTS board_notes_ticket
  ON devagotchi.board_notes (tenant_id, board_id, ticket, seq);
CREATE TABLE IF NOT EXISTS devagotchi.board_events (
  tenant_id TEXT NOT NULL,
  board_id TEXT NOT NULL,
  seq BIGINT NOT NULL,
  at BIGINT NOT NULL,
  actor TEXT NOT NULL,
  action TEXT NOT NULL,
  ticket TEXT,
  data TEXT NOT NULL DEFAULT '{}',
  fence BIGINT,
  prev_hash TEXT NOT NULL,
  hash TEXT NOT NULL,
  PRIMARY KEY (tenant_id, board_id, seq),
  FOREIGN KEY (tenant_id, board_id) REFERENCES devagotchi.board_projects (tenant_id, id)
);
CREATE INDEX IF NOT EXISTS board_events_ticket
  ON devagotchi.board_events (tenant_id, board_id, ticket, seq);
CREATE TABLE IF NOT EXISTS devagotchi.board_imports (
  tenant_id TEXT NOT NULL,
  board_id TEXT NOT NULL,
  source_hash TEXT NOT NULL,
  receipt TEXT NOT NULL,
  PRIMARY KEY (tenant_id, board_id),
  FOREIGN KEY (tenant_id, board_id) REFERENCES devagotchi.board_projects (tenant_id, id)
);
CREATE TABLE IF NOT EXISTS devagotchi.board_outbox (
  tenant_id TEXT NOT NULL,
  board_id TEXT NOT NULL,
  event_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  payload TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  delivered_at BIGINT,
  PRIMARY KEY (tenant_id, board_id, event_id),
  FOREIGN KEY (tenant_id, board_id) REFERENCES devagotchi.board_projects (tenant_id, id)
);
CREATE INDEX IF NOT EXISTS board_outbox_pending ON devagotchi.board_outbox
  (tenant_id, created_at, board_id, event_id) WHERE delivered_at IS NULL;

-- Issues from a repository's tracker, the deliveries that brought them, and the receipts
-- of context handed to an agent.
CREATE TABLE IF NOT EXISTS devagotchi.board_issues (
  tenant_id TEXT NOT NULL,
  board_id TEXT NOT NULL,
  repository_id BIGINT NOT NULL CHECK (repository_id > 0),
  issue_number BIGINT NOT NULL CHECK (issue_number > 0),
  ticket TEXT NOT NULL,
  repository TEXT NOT NULL,
  issue_title TEXT NOT NULL,
  issue_url TEXT NOT NULL,
  source_state TEXT NOT NULL CHECK (source_state IN ('open', 'closed')),
  source_action TEXT NOT NULL,
  source_actor TEXT NOT NULL,
  source_updated_at TEXT NOT NULL,
  source_time BIGINT NOT NULL,
  source_hash TEXT NOT NULL,
  source_revision BIGINT NOT NULL CHECK (source_revision > 0),
  epoch BIGINT NOT NULL CHECK (epoch > 0),
  needs_reconciliation BIGINT NOT NULL DEFAULT 0,
  PRIMARY KEY (tenant_id, board_id, repository_id, issue_number),
  UNIQUE (tenant_id, board_id, ticket),
  FOREIGN KEY (tenant_id, board_id, ticket)
    REFERENCES devagotchi.board_tickets (tenant_id, board_id, id)
);
CREATE TABLE IF NOT EXISTS devagotchi.board_issue_deliveries (
  tenant_id TEXT NOT NULL,
  board_id TEXT NOT NULL,
  repository_id BIGINT NOT NULL,
  delivery_id TEXT NOT NULL,
  payload_hash TEXT NOT NULL,
  issue_number BIGINT NOT NULL,
  outcome TEXT NOT NULL,
  received_at BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, board_id, repository_id, delivery_id),
  FOREIGN KEY (tenant_id, board_id, repository_id, issue_number)
    REFERENCES devagotchi.board_issues (tenant_id, board_id, repository_id, issue_number)
);
CREATE TABLE IF NOT EXISTS devagotchi.board_context_receipts (
  tenant_id TEXT NOT NULL,
  board_id TEXT NOT NULL,
  actor TEXT NOT NULL,
  request_id TEXT NOT NULL,
  payload_hash TEXT NOT NULL,
  ticket TEXT NOT NULL,
  note_seq BIGINT NOT NULL,
  revision BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, board_id, actor, request_id),
  FOREIGN KEY (tenant_id, board_id, ticket)
    REFERENCES devagotchi.board_tickets (tenant_id, board_id, id),
  FOREIGN KEY (tenant_id, board_id, note_seq)
    REFERENCES devagotchi.board_notes (tenant_id, board_id, seq)
);

-- History is append-only.
CREATE OR REPLACE FUNCTION devagotchi.board_append_only() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'board history is append-only';
END $$;
CREATE OR REPLACE TRIGGER board_events_append_only BEFORE UPDATE OR DELETE
  ON devagotchi.board_events FOR EACH ROW EXECUTE FUNCTION devagotchi.board_append_only();
CREATE OR REPLACE TRIGGER board_notes_append_only BEFORE UPDATE OR DELETE
  ON devagotchi.board_notes FOR EACH ROW EXECUTE FUNCTION devagotchi.board_append_only();
CREATE OR REPLACE TRIGGER board_imports_append_only BEFORE UPDATE OR DELETE
  ON devagotchi.board_imports FOR EACH ROW EXECUTE FUNCTION devagotchi.board_append_only();
CREATE OR REPLACE TRIGGER board_issue_deliveries_append_only BEFORE UPDATE OR DELETE
  ON devagotchi.board_issue_deliveries FOR EACH ROW
  EXECUTE FUNCTION devagotchi.board_append_only();
CREATE OR REPLACE TRIGGER board_context_receipts_append_only BEFORE UPDATE OR DELETE
  ON devagotchi.board_context_receipts FOR EACH ROW
  EXECUTE FUNCTION devagotchi.board_append_only();

-- Every board table: a row only inside a transaction scoped to its tenant; the host's
-- role (`devagotchi_hub`, when there is one) may read and write them.
DO $$
DECLARE name text;
BEGIN
  FOREACH name IN ARRAY ARRAY[
    'board_projects', 'board_metadata', 'board_settings', 'board_fence', 'board_tickets',
    'board_deps', 'board_notes', 'board_events', 'board_imports', 'board_outbox',
    'board_issues', 'board_issue_deliveries', 'board_context_receipts'
  ] LOOP
    EXECUTE format('ALTER TABLE devagotchi.%I ENABLE ROW LEVEL SECURITY', name);
    EXECUTE format('ALTER TABLE devagotchi.%I FORCE ROW LEVEL SECURITY', name);
    IF NOT EXISTS (
      SELECT 1 FROM pg_policies
      WHERE schemaname = 'devagotchi' AND tablename = name AND policyname = 'tenant_only'
    ) THEN
      EXECUTE format(
        'CREATE POLICY tenant_only ON devagotchi.%I
         USING (tenant_id = current_setting(''devagotchi.tenant'', true))
         WITH CHECK (tenant_id = current_setting(''devagotchi.tenant'', true))', name);
    END IF;
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub') THEN
      EXECUTE format('GRANT SELECT, INSERT, UPDATE, DELETE ON devagotchi.%I TO devagotchi_hub', name);
    END IF;
  END LOOP;
END $$;

-- The exporter's questions across tenants, answered without reading any tenant's rows:
-- which tenants have board events waiting for ClickHouse, how far behind the export is, and
-- (`devagotchi hub analytics-resend`) send every event again. Each tenant's waiting rows are
-- then read and marked delivered in a transaction scoped to it. They need an owner that
-- bypasses row-level security: the role that runs these migrations.
CREATE OR REPLACE FUNCTION devagotchi.outbox_pending_tenants(most bigint)
RETURNS SETOF text LANGUAGE sql STABLE SECURITY DEFINER SET search_path = devagotchi, pg_temp AS $$
  SELECT tenant_id FROM board_outbox WHERE delivered_at IS NULL
  GROUP BY tenant_id ORDER BY min(created_at), tenant_id LIMIT most
$$;
CREATE OR REPLACE FUNCTION devagotchi.outbox_backlog()
RETURNS TABLE (pending bigint, oldest bigint)
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = devagotchi, pg_temp AS $$
  SELECT count(*)::bigint, min(created_at)::bigint FROM board_outbox WHERE delivered_at IS NULL
$$;
CREATE OR REPLACE FUNCTION devagotchi.outbox_resend()
RETURNS bigint LANGUAGE sql VOLATILE SECURITY DEFINER SET search_path = devagotchi, pg_temp AS $$
  WITH again AS (
    UPDATE board_outbox SET delivered_at = NULL WHERE delivered_at IS NOT NULL RETURNING 1
  )
  SELECT count(*)::bigint FROM again
$$;
REVOKE ALL ON FUNCTION devagotchi.outbox_pending_tenants(bigint) FROM PUBLIC;
REVOKE ALL ON FUNCTION devagotchi.outbox_backlog() FROM PUBLIC;
REVOKE ALL ON FUNCTION devagotchi.outbox_resend() FROM PUBLIC;
DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub') THEN
    GRANT USAGE ON SCHEMA devagotchi TO devagotchi_hub;
    GRANT EXECUTE ON FUNCTION devagotchi.outbox_pending_tenants(bigint) TO devagotchi_hub;
    GRANT EXECUTE ON FUNCTION devagotchi.outbox_backlog() TO devagotchi_hub;
    GRANT EXECUTE ON FUNCTION devagotchi.outbox_resend() TO devagotchi_hub;
  END IF;
END $$;
