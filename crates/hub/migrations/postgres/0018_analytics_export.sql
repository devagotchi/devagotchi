-- The exporter's two questions across tenants (increment 115, warehouse.rs), answered
-- without reading any tenant's rows: which tenants have board events waiting for
-- ClickHouse, and how far behind the export is (how many wait, since when). Each tenant's
-- waiting rows are then read and marked delivered in a transaction scoped to it, under the
-- row-level security of 0014. Kept in step with ../sqlite/0018_analytics_export.sql, which
-- has no functions: the exporter runs on Postgres only.

CREATE FUNCTION devagotchi.outbox_pending_tenants(most bigint)
RETURNS SETOF text LANGUAGE sql STABLE SECURITY DEFINER SET search_path = devagotchi, pg_temp AS $$
  SELECT tenant_id FROM board_outbox WHERE delivered_at IS NULL
  GROUP BY tenant_id ORDER BY min(created_at), tenant_id LIMIT most
$$;

CREATE FUNCTION devagotchi.outbox_backlog()
RETURNS TABLE (pending bigint, oldest bigint)
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = devagotchi, pg_temp AS $$
  SELECT count(*)::bigint, min(created_at)::bigint FROM board_outbox WHERE delivered_at IS NULL
$$;

-- Send every tenant's board events again (`devagotchi hub analytics-resend`): how a
-- ClickHouse that lost its data is filled again from the history Postgres keeps. The
-- warehouse drops what it already has by event id.
CREATE FUNCTION devagotchi.outbox_resend()
RETURNS bigint LANGUAGE sql VOLATILE SECURITY DEFINER SET search_path = devagotchi, pg_temp AS $$
  WITH again AS (
    UPDATE board_outbox SET delivered_at = NULL WHERE delivered_at IS NOT NULL RETURNING 1
  )
  SELECT count(*)::bigint FROM again
$$;

REVOKE ALL ON FUNCTION devagotchi.outbox_pending_tenants(bigint) FROM PUBLIC;
REVOKE ALL ON FUNCTION devagotchi.outbox_backlog() FROM PUBLIC;
REVOKE ALL ON FUNCTION devagotchi.outbox_resend() FROM PUBLIC;
DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub') THEN
    GRANT EXECUTE ON FUNCTION devagotchi.outbox_pending_tenants(bigint) TO devagotchi_hub;
    GRANT EXECUTE ON FUNCTION devagotchi.outbox_backlog() TO devagotchi_hub;
    GRANT EXECUTE ON FUNCTION devagotchi.outbox_resend() TO devagotchi_hub;
  END IF;
END $$;
