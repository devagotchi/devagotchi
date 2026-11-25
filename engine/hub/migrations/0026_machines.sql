-- The machine registry (increment 143, engine/hub/src/machines/devices/): the machines that reach
-- this hub over the link, the one-time codes that join them, and the phones and browsers that
-- get push notifications. Before this they were a SQLite file in the hub's state directory;
-- pre-release, nothing is carried over.
--
-- Hub-wide, like oauth_devices and auth_email_links: the link proves a machine by its name
-- before any tenant is known, and a machine the hub's config names belongs to no account.
-- Who reaches a machine is decided by `owner` (devices.rs, tenant_machines.rs).
-- Postgres only: the hub keeps nothing in SQLite (.ai/dev/plans/postgres-only-hub.md).
CREATE TABLE devagotchi.machine_devices (
  name TEXT PRIMARY KEY,
  public_key TEXT NOT NULL,
  enrolled_at BIGINT NOT NULL,
  revoked_at BIGINT,
  owner TEXT
);
CREATE INDEX machine_devices_owner ON devagotchi.machine_devices (owner);

CREATE TABLE devagotchi.machine_enroll_codes (
  code_hash TEXT PRIMARY KEY,
  device TEXT NOT NULL,
  expires_at BIGINT NOT NULL,
  used_at BIGINT,
  owner TEXT
);
CREATE INDEX machine_enroll_codes_device ON devagotchi.machine_enroll_codes (device);

CREATE TABLE devagotchi.push_subscriptions (
  endpoint TEXT PRIMARY KEY,
  subject TEXT NOT NULL,
  p256dh TEXT NOT NULL,
  auth TEXT NOT NULL,
  created_at BIGINT NOT NULL
);
CREATE INDEX push_subscriptions_subject ON devagotchi.push_subscriptions (subject);

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'devagotchi_hub') THEN
    GRANT SELECT, INSERT, UPDATE, DELETE ON devagotchi.machine_devices,
      devagotchi.machine_enroll_codes, devagotchi.push_subscriptions TO devagotchi_hub;
  END IF;
END $$;
