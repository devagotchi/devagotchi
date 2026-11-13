-- Sign in with Shippin (Nomen) and any OpenID Connect provider from the hub's settings page
-- (increment 134, accounts/sign_in_settings.rs): a provider of kind oidc, with its issuer and
-- the label its button shows. Existing rows keep their kind and get empty values. Kept in step
-- with ../sqlite/0023_sign_in_oidc.sql.
ALTER TABLE devagotchi.sign_in_settings ADD COLUMN issuer TEXT NOT NULL DEFAULT '';
ALTER TABLE devagotchi.sign_in_settings ADD COLUMN label TEXT NOT NULL DEFAULT '';
ALTER TABLE devagotchi.sign_in_settings DROP CONSTRAINT IF EXISTS sign_in_settings_kind_check;
ALTER TABLE devagotchi.sign_in_settings ADD CONSTRAINT sign_in_settings_kind_check
  CHECK (kind IN ('google', 'github', 'oidc'));
