-- The hub's audit chain (increment 147, engine/hub/src/audit.rs): one row per routed call,
-- each hash-chained to the one before, so an edited or truncated record is visible on
-- /healthz. Before this it was audit.sqlite3 in the state directory; pre-release, nothing is
-- carried over. Postgres only (.ai/dev/plans/free-edition.md, R3).
--
-- Hub-wide: the operator's record of every call, whoever made it. Appended under
-- pg_advisory_xact_lock (audit.rs), so several hub instances keep one chain. The hub's role
-- may read and append, never change or remove a row.
CREATE TABLE devagotchi.audit_entries (
  seq BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  at TEXT NOT NULL,
  subject TEXT NOT NULL,
  via TEXT NOT NULL,
  tool TEXT NOT NULL,
  broker TEXT NOT NULL,
  operation_id TEXT NOT NULL,
  outcome TEXT NOT NULL,
  prev_hash TEXT NOT NULL,
  hash TEXT NOT NULL
);

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub') THEN
    GRANT SELECT, INSERT ON devagotchi.audit_entries TO devagotchi_hub;
    EXECUTE format('GRANT USAGE ON SEQUENCE %s TO devagotchi_hub',
      pg_get_serial_sequence('devagotchi.audit_entries', 'seq'));
  END IF;
END $$;
