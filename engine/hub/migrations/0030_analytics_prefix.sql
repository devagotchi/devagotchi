-- The ops domain's first-party analytics carry its prefix (.ai/patterns/layout.md rule 8):
-- hub_analytics_views, hub_analytics_events and hub_analytics_daily become analytics_views,
-- analytics_events and analytics_daily, beside analytics_settings, with their indexes and
-- constraints. Their grants go with each table. Pre-release: nothing reads the old names.

ALTER TABLE devagotchi.hub_analytics_views RENAME TO analytics_views;
ALTER TABLE devagotchi.hub_analytics_events RENAME TO analytics_events;
ALTER TABLE devagotchi.hub_analytics_daily RENAME TO analytics_daily;

-- Constraints (a primary key renames its index with it), then the indexes left, named after
-- their table's old name.
DO $$
DECLARE
  renamed record;
  found record;
BEGIN
  FOR renamed IN
    SELECT * FROM (VALUES
      ('hub_analytics_views', 'analytics_views'),
      ('hub_analytics_events', 'analytics_events'),
      ('hub_analytics_daily', 'analytics_daily')
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
