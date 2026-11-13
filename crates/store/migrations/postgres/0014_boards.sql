-- Hosted board storage. Local board files are not imported or switched automatically.
CREATE TABLE devagotchi.board_projects (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants (id),
  id TEXT NOT NULL,
  source_hash TEXT NOT NULL,
  imported_at BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, id)
);
CREATE TABLE devagotchi.board_metadata (
  tenant_id TEXT NOT NULL,
  board_id TEXT NOT NULL,
  key TEXT NOT NULL,
  value TEXT NOT NULL,
  PRIMARY KEY (tenant_id, board_id, key),
  FOREIGN KEY (tenant_id, board_id) REFERENCES devagotchi.board_projects (tenant_id, id)
);
CREATE TABLE devagotchi.board_settings (
  tenant_id TEXT NOT NULL,
  board_id TEXT NOT NULL,
  key TEXT NOT NULL,
  value TEXT NOT NULL,
  PRIMARY KEY (tenant_id, board_id, key),
  FOREIGN KEY (tenant_id, board_id) REFERENCES devagotchi.board_projects (tenant_id, id)
);
CREATE TABLE devagotchi.board_fence (
  tenant_id TEXT NOT NULL,
  board_id TEXT NOT NULL,
  id BIGINT NOT NULL CHECK (id = 1),
  value BIGINT NOT NULL CHECK (value >= 0),
  PRIMARY KEY (tenant_id, board_id, id),
  FOREIGN KEY (tenant_id, board_id) REFERENCES devagotchi.board_projects (tenant_id, id)
);
CREATE TABLE devagotchi.board_tickets (
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
CREATE INDEX board_tickets_queue ON devagotchi.board_tickets
  (tenant_id, board_id, state, priority, created_at) WHERE holder IS NULL;
CREATE INDEX board_tickets_lease ON devagotchi.board_tickets
  (tenant_id, board_id, lease_expires_at) WHERE holder IS NOT NULL;
CREATE INDEX board_tickets_parent ON devagotchi.board_tickets (tenant_id, board_id, parent);
CREATE INDEX board_tickets_fingerprint ON devagotchi.board_tickets
  (tenant_id, board_id, fingerprint);
CREATE TABLE devagotchi.board_deps (
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
CREATE INDEX board_deps_reverse ON devagotchi.board_deps (tenant_id, board_id, depends_on);
CREATE TABLE devagotchi.board_notes (
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
CREATE INDEX board_notes_ticket ON devagotchi.board_notes (tenant_id, board_id, ticket, seq);
CREATE TABLE devagotchi.board_events (
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
CREATE INDEX board_events_ticket ON devagotchi.board_events (tenant_id, board_id, ticket, seq);
CREATE TABLE devagotchi.board_imports (
  tenant_id TEXT NOT NULL,
  board_id TEXT NOT NULL,
  source_hash TEXT NOT NULL,
  receipt TEXT NOT NULL,
  PRIMARY KEY (tenant_id, board_id),
  FOREIGN KEY (tenant_id, board_id) REFERENCES devagotchi.board_projects (tenant_id, id)
);
CREATE TABLE devagotchi.board_outbox (
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
CREATE INDEX board_outbox_pending ON devagotchi.board_outbox
  (tenant_id, created_at, board_id, event_id) WHERE delivered_at IS NULL;

CREATE FUNCTION devagotchi.board_append_only() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'board history is append-only';
END $$;
CREATE TRIGGER board_events_append_only BEFORE UPDATE OR DELETE ON devagotchi.board_events
  FOR EACH ROW EXECUTE FUNCTION devagotchi.board_append_only();
CREATE TRIGGER board_notes_append_only BEFORE UPDATE OR DELETE ON devagotchi.board_notes
  FOR EACH ROW EXECUTE FUNCTION devagotchi.board_append_only();
CREATE TRIGGER board_imports_append_only BEFORE UPDATE OR DELETE ON devagotchi.board_imports
  FOR EACH ROW EXECUTE FUNCTION devagotchi.board_append_only();

DO $$
DECLARE name text;
BEGIN
  FOREACH name IN ARRAY ARRAY[
    'board_projects', 'board_metadata', 'board_settings', 'board_fence', 'board_tickets',
    'board_deps', 'board_notes', 'board_events', 'board_imports', 'board_outbox'
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
