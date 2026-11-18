-- AgentHub's exporter functions (its migration 0001: outbox_pending_tenants, outbox_backlog,
-- outbox_resend) for this hub's role. AgentHub grants them to `devagotchi_hub` only if that
-- role exists when its migrations run, and a role is the server's, made by this hub (0007).
-- Where AgentHub's migrations ran first on a server with no such role yet (`devagotchi board
-- init` on the database before this hub first started on it, as tests/e2e.py does), they are
-- there without the grant and every export is refused: "permission denied for function
-- outbox_pending_tenants". 0007 grants this role every table it finds, never a function; these
-- three are granted here when both are there. On a new database AgentHub's tables and
-- functions come after this, and its 0001 grants them. Add-only.

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub')
     AND to_regprocedure('devagotchi.outbox_pending_tenants(bigint)') IS NOT NULL THEN
    GRANT EXECUTE ON FUNCTION devagotchi.outbox_pending_tenants(bigint) TO devagotchi_hub;
    GRANT EXECUTE ON FUNCTION devagotchi.outbox_backlog() TO devagotchi_hub;
    GRANT EXECUTE ON FUNCTION devagotchi.outbox_resend() TO devagotchi_hub;
  END IF;
END $$;
