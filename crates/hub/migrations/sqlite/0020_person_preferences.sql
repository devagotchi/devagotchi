-- What a person told the hub they want (increment 119, preferences.rs): SQLite's half of
-- ../postgres/0020_person_preferences.sql. Same table and columns; no row-level security and
-- no function here, so preferences.rs answers `machine_pushes_of` with a plain query on this
-- backend. Add-only.
CREATE TABLE person_preferences (
  tenant_id TEXT NOT NULL REFERENCES tenancy_tenants (id),
  subject TEXT NOT NULL,
  schema_version INTEGER NOT NULL,
  body TEXT NOT NULL,
  machine_pushes INTEGER NOT NULL DEFAULT 1,
  updated_at BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, subject)
);
CREATE INDEX person_preferences_by_subject ON person_preferences (subject);
