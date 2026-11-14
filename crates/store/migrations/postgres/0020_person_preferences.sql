-- What a person told the hub they want (increment 119, preferences.rs): one row per person,
-- under their tenant's row-level security. `body` is the JSON of their preferences and
-- `schema_version` says which shape it has, so a later hub can upgrade an old row as it reads
-- it and an older hub keeps what it does not know. `machine_pushes` repeats one preference
-- as a column, for the one question asked without a tenant: may this person's machines buzz
-- their phone. Add-only. Kept in step with ../sqlite/0020_person_preferences.sql.
CREATE TABLE devagotchi.person_preferences (
  tenant_id TEXT NOT NULL REFERENCES devagotchi.tenancy_tenants (id),
  subject TEXT NOT NULL,
  schema_version INTEGER NOT NULL,
  body TEXT NOT NULL,
  machine_pushes INTEGER NOT NULL DEFAULT 1,
  updated_at BIGINT NOT NULL,
  PRIMARY KEY (tenant_id, subject)
);
CREATE INDEX person_preferences_by_subject ON devagotchi.person_preferences (subject);

ALTER TABLE devagotchi.person_preferences ENABLE ROW LEVEL SECURITY;
ALTER TABLE devagotchi.person_preferences FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_only ON devagotchi.person_preferences
  USING (tenant_id = current_setting('devagotchi.tenant', true))
  WITH CHECK (tenant_id = current_setting('devagotchi.tenant', true));

-- Whether a machine may push to this person, across tenants, and nothing else: 1 unless a row
-- of theirs turned it off. A machine knows its owner's subject, not the owner's tenant.
CREATE FUNCTION devagotchi.machine_pushes_of(who text)
RETURNS integer LANGUAGE sql STABLE SECURITY DEFINER SET search_path = devagotchi, pg_temp AS $$
  SELECT COALESCE(MIN(machine_pushes), 1) FROM person_preferences WHERE subject = who
$$;
REVOKE ALL ON FUNCTION devagotchi.machine_pushes_of(text) FROM PUBLIC;

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub') THEN
    GRANT SELECT, INSERT, UPDATE, DELETE ON devagotchi.person_preferences TO devagotchi_hub;
    GRANT EXECUTE ON FUNCTION devagotchi.machine_pushes_of(text) TO devagotchi_hub;
  END IF;
END $$;
