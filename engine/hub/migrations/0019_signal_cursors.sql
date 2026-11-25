-- Where each signal sink is up to (increment 117, signals.rs): one row per sink, source and
-- scope (a tenant for `board` and `tenancy`, '' for the hub's `audit` chain), holding the
-- position of the last signal the sink took. The sources themselves are append-only
-- (board_outbox, tenancy_events, the audit chain), so a sink that is down only falls behind:
-- nothing is dropped and no other sink waits for it. A hub's own bookkeeping, like
-- tenancy_tenants: ids and positions only, no tenant's data. Kept in step with
-- ../sqlite/0019_signal_cursors.sql.
CREATE TABLE devagotchi.signal_cursors (
  sink TEXT NOT NULL,
  source TEXT NOT NULL CHECK (source IN ('board', 'tenancy', 'audit')),
  scope TEXT NOT NULL,
  position TEXT NOT NULL,
  updated_at BIGINT NOT NULL,
  PRIMARY KEY (sink, source, scope)
);

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub') THEN
    GRANT SELECT, INSERT, UPDATE, DELETE ON devagotchi.signal_cursors TO devagotchi_hub;
  END IF;
END $$;
