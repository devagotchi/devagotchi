-- Where each signal sink is up to (increment 117, signals.rs): SQLite's half of
-- ../postgres/0019_signal_cursors.sql. The sinks run on Postgres; the table is kept in step.
CREATE TABLE signal_cursors (
  sink TEXT NOT NULL,
  source TEXT NOT NULL CHECK (source IN ('board', 'tenancy', 'audit')),
  scope TEXT NOT NULL,
  position TEXT NOT NULL,
  updated_at BIGINT NOT NULL,
  PRIMARY KEY (sink, source, scope)
);
