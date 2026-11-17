-- The lexicon a person keeps through the hub (kernel K3, crates/hub/src/kernel/): their own
-- phrases and their projects', what each means and where it stands. The rules that move an
-- entry are the kernel's (crates/kernel/src/lexicon/); these tables only keep what it decides.
--
-- kernel_lexicon_entries: one row per entry, keyed by its owner (the signed-in subject, never
--   what a tool call said) and its id. `entry` is the entry as the kernel writes it (JSON);
--   scope, normalized phrase, state and revision are kept beside it so the database holds the
--   two rules a racing write could break: the revision a change read, and one live entry
--   (proposed, confirmed or inconclusive) per phrase in one scope.
-- kernel_lexicon_events: what each change left, oldest first by `seq`.
-- kernel_lexicon_receipts: the first answer to each request id, so a retry gets it again.
--
-- Hub-wide, like machine_devices: a subject reaches the hub with or without a tenant (a dev
-- token, a Nomen sign-in), so every query names the owner instead of a tenant's row-level
-- security. Deleting an entry erases its words from its events and receipts too. Add-only.

CREATE TABLE devagotchi.kernel_lexicon_entries (
  owner_id TEXT NOT NULL,
  id TEXT NOT NULL,
  scope_kind TEXT NOT NULL CHECK (scope_kind IN ('personal', 'project')),
  project_id TEXT NOT NULL DEFAULT '',
  phrase_key TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('proposed', 'confirmed', 'inconclusive', 'dismissed',
    'retired', 'deleted')),
  revision BIGINT NOT NULL CHECK (revision > 0),
  entry TEXT NOT NULL,
  PRIMARY KEY (owner_id, id)
);
CREATE INDEX kernel_lexicon_entries_scope
  ON devagotchi.kernel_lexicon_entries (owner_id, scope_kind, project_id, phrase_key, id);
CREATE UNIQUE INDEX kernel_lexicon_entries_live_phrase
  ON devagotchi.kernel_lexicon_entries (owner_id, scope_kind, project_id, phrase_key)
  WHERE state IN ('proposed', 'confirmed', 'inconclusive');

CREATE TABLE devagotchi.kernel_lexicon_events (
  seq BIGSERIAL PRIMARY KEY,
  owner_id TEXT NOT NULL,
  entry_id TEXT NOT NULL,
  event TEXT NOT NULL
);
CREATE INDEX kernel_lexicon_events_entry
  ON devagotchi.kernel_lexicon_events (owner_id, entry_id, seq);

CREATE TABLE devagotchi.kernel_lexicon_receipts (
  owner_id TEXT NOT NULL,
  request_id TEXT NOT NULL,
  entry_id TEXT NOT NULL,
  payload_hash TEXT NOT NULL,
  entry TEXT NOT NULL,
  PRIMARY KEY (owner_id, request_id)
);
CREATE INDEX kernel_lexicon_receipts_entry
  ON devagotchi.kernel_lexicon_receipts (owner_id, entry_id);

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub') THEN
    GRANT SELECT, INSERT, UPDATE, DELETE ON devagotchi.kernel_lexicon_entries,
      devagotchi.kernel_lexicon_events, devagotchi.kernel_lexicon_receipts TO devagotchi_hub;
    EXECUTE format('GRANT USAGE ON SEQUENCE %s TO devagotchi_hub',
      pg_get_serial_sequence('devagotchi.kernel_lexicon_events', 'seq'));
  END IF;
END $$;
