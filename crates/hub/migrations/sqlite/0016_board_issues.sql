-- Storage-contract fixture only. Hosted issue integration requires Postgres.
CREATE TABLE board_issues (
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
  FOREIGN KEY (tenant_id, board_id, ticket) REFERENCES board_tickets (tenant_id, board_id, id)
);
CREATE TABLE board_issue_deliveries (
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
    REFERENCES board_issues (tenant_id, board_id, repository_id, issue_number)
);
CREATE TABLE board_context_receipts (
  tenant_id TEXT NOT NULL,
  board_id TEXT NOT NULL,
  actor TEXT NOT NULL,
  request_id TEXT NOT NULL,
  payload_hash TEXT NOT NULL,
  ticket TEXT NOT NULL,
  note_seq BIGINT NOT NULL,
  revision BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, board_id, actor, request_id),
  FOREIGN KEY (tenant_id, board_id, ticket) REFERENCES board_tickets (tenant_id, board_id, id),
  FOREIGN KEY (tenant_id, board_id, note_seq) REFERENCES board_notes (tenant_id, board_id, seq)
);
CREATE TRIGGER board_issue_deliveries_append_only_update BEFORE UPDATE ON board_issue_deliveries
BEGIN SELECT RAISE(ABORT, 'board history is append-only'); END;
CREATE TRIGGER board_issue_deliveries_append_only_delete BEFORE DELETE ON board_issue_deliveries
BEGIN SELECT RAISE(ABORT, 'board history is append-only'); END;
CREATE TRIGGER board_context_receipts_append_only_update BEFORE UPDATE ON board_context_receipts
BEGIN SELECT RAISE(ABORT, 'board history is append-only'); END;
CREATE TRIGGER board_context_receipts_append_only_delete BEFORE DELETE ON board_context_receipts
BEGIN SELECT RAISE(ABORT, 'board history is append-only'); END;
