-- Same hosted-board schema as Postgres, used by storage-contract tests. Local CLI
-- board files retain their own schema; this migration does not switch a board's owner.
CREATE TABLE board_projects (
  tenant_id TEXT NOT NULL REFERENCES tenancy_tenants (id),
  id TEXT NOT NULL,
  source_hash TEXT NOT NULL,
  imported_at BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, id)
);
CREATE TABLE board_metadata (
  tenant_id TEXT NOT NULL, board_id TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL,
  PRIMARY KEY (tenant_id, board_id, key),
  FOREIGN KEY (tenant_id, board_id) REFERENCES board_projects (tenant_id, id)
);
CREATE TABLE board_settings (
  tenant_id TEXT NOT NULL, board_id TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL,
  PRIMARY KEY (tenant_id, board_id, key),
  FOREIGN KEY (tenant_id, board_id) REFERENCES board_projects (tenant_id, id)
);
CREATE TABLE board_fence (
  tenant_id TEXT NOT NULL, board_id TEXT NOT NULL,
  id BIGINT NOT NULL CHECK (id = 1), value BIGINT NOT NULL CHECK (value >= 0),
  PRIMARY KEY (tenant_id, board_id, id),
  FOREIGN KEY (tenant_id, board_id) REFERENCES board_projects (tenant_id, id)
);
CREATE TABLE board_tickets (
  tenant_id TEXT NOT NULL, board_id TEXT NOT NULL,
  id TEXT NOT NULL, kind TEXT NOT NULL DEFAULT 'task', title TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '', state TEXT NOT NULL, priority BIGINT NOT NULL DEFAULT 2,
  labels TEXT NOT NULL DEFAULT '[]', parent TEXT, child_seq BIGINT NOT NULL DEFAULT 0,
  assignee TEXT, holder TEXT, lease_id TEXT, lease_ttl_ms BIGINT, lease_expires_at BIGINT,
  heartbeat_at BIGINT, fence BIGINT NOT NULL DEFAULT 0, version BIGINT NOT NULL DEFAULT 1,
  expiry_count BIGINT NOT NULL DEFAULT 0, last_worker TEXT, progress TEXT,
  created_by TEXT NOT NULL, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
  closed_at BIGINT, machine TEXT, repo TEXT, environment TEXT, agent TEXT, session TEXT,
  fingerprint TEXT, occurrences BIGINT NOT NULL DEFAULT 1, last_seen_at BIGINT,
  PRIMARY KEY (tenant_id, board_id, id),
  FOREIGN KEY (tenant_id, board_id) REFERENCES board_projects (tenant_id, id),
  FOREIGN KEY (tenant_id, board_id, parent) REFERENCES board_tickets (tenant_id, board_id, id)
    DEFERRABLE INITIALLY DEFERRED,
  CHECK ((holder IS NULL) = (lease_id IS NULL))
);
CREATE INDEX board_tickets_queue ON board_tickets
  (tenant_id, board_id, state, priority, created_at) WHERE holder IS NULL;
CREATE INDEX board_tickets_lease ON board_tickets
  (tenant_id, board_id, lease_expires_at) WHERE holder IS NOT NULL;
CREATE INDEX board_tickets_parent ON board_tickets (tenant_id, board_id, parent);
CREATE INDEX board_tickets_fingerprint ON board_tickets (tenant_id, board_id, fingerprint);
CREATE TABLE board_deps (
  tenant_id TEXT NOT NULL, board_id TEXT NOT NULL, ticket TEXT NOT NULL,
  depends_on TEXT NOT NULL, kind TEXT NOT NULL DEFAULT 'blocks',
  created_by TEXT NOT NULL, created_at BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, board_id, ticket, depends_on, kind),
  FOREIGN KEY (tenant_id, board_id, ticket) REFERENCES board_tickets (tenant_id, board_id, id),
  FOREIGN KEY (tenant_id, board_id, depends_on) REFERENCES board_tickets (tenant_id, board_id, id),
  CHECK (ticket <> depends_on)
);
CREATE INDEX board_deps_reverse ON board_deps (tenant_id, board_id, depends_on);
CREATE TABLE board_notes (
  tenant_id TEXT NOT NULL, board_id TEXT NOT NULL, seq BIGINT NOT NULL,
  ticket TEXT NOT NULL, author TEXT NOT NULL, kind TEXT NOT NULL, body TEXT NOT NULL,
  to_principal TEXT, fence BIGINT, created_at BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, board_id, seq),
  FOREIGN KEY (tenant_id, board_id, ticket) REFERENCES board_tickets (tenant_id, board_id, id)
);
CREATE INDEX board_notes_ticket ON board_notes (tenant_id, board_id, ticket, seq);
CREATE TABLE board_events (
  tenant_id TEXT NOT NULL, board_id TEXT NOT NULL, seq BIGINT NOT NULL, at BIGINT NOT NULL,
  actor TEXT NOT NULL, action TEXT NOT NULL, ticket TEXT, data TEXT NOT NULL DEFAULT '{}',
  fence BIGINT, prev_hash TEXT NOT NULL, hash TEXT NOT NULL,
  PRIMARY KEY (tenant_id, board_id, seq),
  FOREIGN KEY (tenant_id, board_id) REFERENCES board_projects (tenant_id, id)
);
CREATE INDEX board_events_ticket ON board_events (tenant_id, board_id, ticket, seq);
CREATE TABLE board_imports (
  tenant_id TEXT NOT NULL, board_id TEXT NOT NULL, source_hash TEXT NOT NULL,
  receipt TEXT NOT NULL,
  PRIMARY KEY (tenant_id, board_id),
  FOREIGN KEY (tenant_id, board_id) REFERENCES board_projects (tenant_id, id)
);
CREATE TABLE board_outbox (
  tenant_id TEXT NOT NULL, board_id TEXT NOT NULL, event_id TEXT NOT NULL,
  kind TEXT NOT NULL, payload TEXT NOT NULL, created_at BIGINT NOT NULL, delivered_at BIGINT,
  PRIMARY KEY (tenant_id, board_id, event_id),
  FOREIGN KEY (tenant_id, board_id) REFERENCES board_projects (tenant_id, id)
);
CREATE INDEX board_outbox_pending ON board_outbox
  (tenant_id, created_at, board_id, event_id) WHERE delivered_at IS NULL;
CREATE TRIGGER board_events_no_update BEFORE UPDATE ON board_events
  BEGIN SELECT RAISE(ABORT, 'board history is append-only'); END;
CREATE TRIGGER board_events_no_delete BEFORE DELETE ON board_events
  BEGIN SELECT RAISE(ABORT, 'board history is append-only'); END;
CREATE TRIGGER board_notes_no_update BEFORE UPDATE ON board_notes
  BEGIN SELECT RAISE(ABORT, 'board history is append-only'); END;
CREATE TRIGGER board_notes_no_delete BEFORE DELETE ON board_notes
  BEGIN SELECT RAISE(ABORT, 'board history is append-only'); END;
CREATE TRIGGER board_imports_no_update BEFORE UPDATE ON board_imports
  BEGIN SELECT RAISE(ABORT, 'board history is append-only'); END;
CREATE TRIGGER board_imports_no_delete BEFORE DELETE ON board_imports
  BEGIN SELECT RAISE(ABORT, 'board history is append-only'); END;
