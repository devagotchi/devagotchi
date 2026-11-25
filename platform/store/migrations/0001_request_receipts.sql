CREATE TABLE devagotchi.store_request_receipts (
  tenant_id TEXT NOT NULL,
  principal TEXT NOT NULL,
  request_key TEXT NOT NULL CHECK (octet_length(request_key) BETWEEN 1 AND 200),
  operation TEXT NOT NULL,
  request_hash TEXT NOT NULL,
  response TEXT,
  created_at BIGINT NOT NULL DEFAULT (floor(extract(epoch FROM clock_timestamp())*1000)::bigint),
  completed_at BIGINT,
  retired_at BIGINT,
  PRIMARY KEY (tenant_id,principal,request_key),
  CHECK ((completed_at IS NULL AND response IS NULL AND retired_at IS NULL)
    OR (completed_at IS NOT NULL AND response IS NOT NULL AND retired_at IS NULL)
    OR (completed_at IS NOT NULL AND response IS NULL AND retired_at IS NOT NULL))
);
CREATE INDEX store_request_receipts_retention
  ON devagotchi.store_request_receipts (tenant_id,completed_at)
  WHERE retired_at IS NULL AND completed_at IS NOT NULL;
ALTER TABLE devagotchi.store_request_receipts ENABLE ROW LEVEL SECURITY;
ALTER TABLE devagotchi.store_request_receipts FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_only ON devagotchi.store_request_receipts
  USING (tenant_id=current_setting('devagotchi.tenant',true))
  WITH CHECK (tenant_id=current_setting('devagotchi.tenant',true));

-- A forgotten completion must roll back the domain mutation, never leave an ambiguous
-- committed reservation. Deferred validation reads the final row, not the insert image.
CREATE FUNCTION devagotchi.store_receipt_complete() RETURNS trigger LANGUAGE plpgsql
SECURITY DEFINER SET search_path = pg_catalog, devagotchi AS $$
BEGIN
  IF EXISTS (SELECT 1 FROM devagotchi.store_request_receipts
      WHERE tenant_id=NEW.tenant_id AND principal=NEW.principal AND request_key=NEW.request_key
        AND completed_at IS NULL) THEN
    RAISE EXCEPTION 'request receipt must complete in its mutation transaction' USING ERRCODE='23514';
  END IF;
  RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER store_receipt_complete
  AFTER INSERT OR UPDATE ON devagotchi.store_request_receipts
  DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION devagotchi.store_receipt_complete();

-- A mistaken writer cannot forget or recycle a committed request identity.
CREATE FUNCTION devagotchi.store_receipt_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP='DELETE' THEN
    RAISE EXCEPTION 'request receipt identities cannot be deleted' USING ERRCODE='23514';
  END IF;
  IF (NEW.tenant_id,NEW.principal,NEW.request_key,NEW.operation,NEW.request_hash,NEW.created_at)
      IS DISTINCT FROM
     (OLD.tenant_id,OLD.principal,OLD.request_key,OLD.operation,OLD.request_hash,OLD.created_at)
     OR OLD.retired_at IS NOT NULL
     OR (OLD.completed_at IS NOT NULL AND
         (NEW.retired_at IS NULL OR NEW.response IS NOT NULL OR NEW.completed_at IS DISTINCT FROM OLD.completed_at)) THEN
    RAISE EXCEPTION 'request receipt identity or outcome is immutable' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER store_receipt_immutable BEFORE UPDATE OR DELETE
  ON devagotchi.store_request_receipts FOR EACH ROW EXECUTE FUNCTION devagotchi.store_receipt_immutable();
DO $$ BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname='devagotchi_hub') THEN
    GRANT SELECT, INSERT, UPDATE ON devagotchi.store_request_receipts TO devagotchi_hub;
  END IF;
END $$;
