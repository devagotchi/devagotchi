CREATE TABLE devagotchi.board_issues (
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
CREATE TABLE devagotchi.board_issue_deliveries (
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
CREATE TABLE devagotchi.board_context_receipts (
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
CREATE TRIGGER board_issue_deliveries_append_only BEFORE UPDATE OR DELETE
  ON devagotchi.board_issue_deliveries FOR EACH ROW
  EXECUTE FUNCTION devagotchi.board_append_only();
CREATE TRIGGER board_context_receipts_append_only BEFORE UPDATE OR DELETE
  ON devagotchi.board_context_receipts FOR EACH ROW
  EXECUTE FUNCTION devagotchi.board_append_only();
DO $$
DECLARE name text;
BEGIN
  FOREACH name IN ARRAY ARRAY['board_issues', 'board_issue_deliveries', 'board_context_receipts']
  LOOP
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
