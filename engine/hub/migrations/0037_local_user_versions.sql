-- Optimistic owner edits of existing human accounts. Identity and tenancy stay unchanged.
ALTER TABLE devagotchi.auth_accounts
  ADD COLUMN management_version BIGINT NOT NULL DEFAULT 1 CHECK (management_version > 0);
