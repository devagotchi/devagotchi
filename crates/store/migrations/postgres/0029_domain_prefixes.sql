-- Every table carries its domain's prefix (.ai/patterns/layout.md rule 8): the five made
-- before the rule are renamed, with their indexes and constraints, and the two lookups that
-- name them are made again. Row-level security, its policies and the grants go with each
-- table. Identity: auth_passwords, auth_preferences, auth_api_keys, auth_sign_in_settings;
-- connectors: connector_keys (a tenant's sealed key for an outside service).

ALTER TABLE devagotchi.passwords RENAME TO auth_passwords;
ALTER TABLE devagotchi.person_preferences RENAME TO auth_preferences;
ALTER TABLE devagotchi.api_keys RENAME TO auth_api_keys;
ALTER TABLE devagotchi.sign_in_settings RENAME TO auth_sign_in_settings;
ALTER TABLE devagotchi.connections RENAME TO connector_keys;

-- Constraints (a primary key renames its index with it), then the indexes left, named
-- after their table's old name.
DO $$
DECLARE
  renamed record;
  found record;
BEGIN
  FOR renamed IN
    SELECT * FROM (VALUES
      ('passwords', 'auth_passwords'),
      ('person_preferences', 'auth_preferences'),
      ('api_keys', 'auth_api_keys'),
      ('sign_in_settings', 'auth_sign_in_settings'),
      ('connections', 'connector_keys')
    ) AS pair (old, new)
  LOOP
    FOR found IN
      SELECT c.conname FROM pg_constraint c
      JOIN pg_class t ON t.oid = c.conrelid
      JOIN pg_namespace n ON n.oid = t.relnamespace
      WHERE n.nspname = 'devagotchi' AND t.relname = renamed.new
        AND c.conname LIKE renamed.old || '\_%'
    LOOP
      EXECUTE format('ALTER TABLE devagotchi.%I RENAME CONSTRAINT %I TO %I', renamed.new,
        found.conname, renamed.new || substr(found.conname, length(renamed.old) + 1));
    END LOOP;
    FOR found IN
      SELECT i.relname FROM pg_index x
      JOIN pg_class i ON i.oid = x.indexrelid
      JOIN pg_class t ON t.oid = x.indrelid
      JOIN pg_namespace n ON n.oid = t.relnamespace
      WHERE n.nspname = 'devagotchi' AND t.relname = renamed.new
        AND i.relname LIKE renamed.old || '\_%'
    LOOP
      EXECUTE format('ALTER INDEX devagotchi.%I RENAME TO %I', found.relname,
        renamed.new || substr(found.relname, length(renamed.old) + 1));
    END LOOP;
  END LOOP;
END $$;

CREATE OR REPLACE FUNCTION devagotchi.machine_pushes_of(who text)
RETURNS integer LANGUAGE sql STABLE SECURITY DEFINER SET search_path = devagotchi, pg_temp AS $$
  SELECT COALESCE(MIN(machine_pushes), 1) FROM auth_preferences WHERE subject = who
$$;

CREATE OR REPLACE FUNCTION devagotchi.api_key_tenant(key_hash text)
RETURNS text LANGUAGE sql STABLE SECURITY DEFINER SET search_path = devagotchi, pg_temp AS $$
  SELECT tenant_id FROM auth_api_keys WHERE hash = key_hash
$$;
