-- Who holds each person's lexicon (kernel K5, crates/hub/src/kernel/authority.rs): this hub,
-- or Clyffy after a handover, one writer at a time. The moves and their rules are the
-- kernel's (crates/kernel/src/handoff/authority.rs); this table keeps where each owner stands.
--
-- kernel_lexicon_authority: one row per owner who has written or moved their lexicon.
--   holder: devagotchi (this hub writes it), handing_over, clyffy (this hub keeps a read-only
--   mirror), restoring. epoch: raised by every move but a completion; a write or a bundle
--   from an older epoch is refused. No row means this hub holds it at epoch 1.
--
-- Every lexicon write takes this row FOR SHARE in its transaction and every move takes it
-- FOR UPDATE, so no write lands after a handover began. Hub-wide and keyed by owner, like
-- the lexicon's own tables (0032). Add-only.

CREATE TABLE devagotchi.kernel_lexicon_authority (
  owner_id TEXT PRIMARY KEY,
  holder TEXT NOT NULL CHECK (holder IN ('devagotchi', 'handing_over', 'clyffy', 'restoring')),
  epoch BIGINT NOT NULL CHECK (epoch > 0)
);

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub') THEN
    GRANT SELECT, INSERT, UPDATE ON devagotchi.kernel_lexicon_authority TO devagotchi_hub;
  END IF;
END $$;
