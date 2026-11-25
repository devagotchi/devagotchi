-- A board transaction stays scoped to its organization. Its host separately checks
-- the exact project role; this function reveals only current account liveness.
CREATE FUNCTION devagotchi.request_account_live(who text, idle_seconds bigint)
RETURNS boolean LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = devagotchi, pg_temp AS $$
  SELECT EXISTS (
    SELECT 1 FROM auth_accounts WHERE id = who AND disabled_at IS NULL
      AND (provisional = 0 OR
        coalesce(last_active, 0) + idle_seconds > extract(epoch FROM statement_timestamp())::bigint)
  )
$$;
REVOKE ALL ON FUNCTION devagotchi.request_account_live(text, bigint) FROM PUBLIC;
DO $$ BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub') THEN
    GRANT EXECUTE ON FUNCTION devagotchi.request_account_live(text, bigint) TO devagotchi_hub;
  END IF;
END $$;
