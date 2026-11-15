-- Nomen sign-in's two stores (increment 147): the browser sessions of the Nomen web login
-- (web/sessions.rs) and the sign-ins Nomen confirmed, kept so a hub restart during a Nomen
-- outage signs nobody out (nomen/kept.rs). Before this they were sessions.sqlite3 and
-- signins.sqlite3 in the state directory; pre-release, nothing is carried over. Postgres only
-- (.ai/dev/plans/free-edition.md, R3).
--
-- Hub-wide: a browser session and a Nomen token are looked up by their hash before any tenant
-- is known. No token is stored in the clear: a session's tokens are sealed under session.key
-- in the state directory and bound to the row's key; a kept sign-in is the token's SHA-256.
CREATE TABLE devagotchi.auth_web_sessions (
  id_hash BYTEA PRIMARY KEY,
  sealed BYTEA NOT NULL,
  access_until BIGINT NOT NULL,
  created_at BIGINT NOT NULL,
  last_seen BIGINT NOT NULL,
  expires_at BIGINT NOT NULL
);
CREATE INDEX auth_web_sessions_last_seen ON devagotchi.auth_web_sessions (last_seen);

CREATE TABLE devagotchi.auth_nomen_sign_ins (
  digest BYTEA PRIMARY KEY,
  subject TEXT NOT NULL,
  name TEXT,
  org TEXT,
  confirmed_at BIGINT NOT NULL,
  honour_until BIGINT NOT NULL
);

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub') THEN
    GRANT SELECT, INSERT, UPDATE, DELETE ON devagotchi.auth_web_sessions,
      devagotchi.auth_nomen_sign_ins TO devagotchi_hub;
  END IF;
END $$;
