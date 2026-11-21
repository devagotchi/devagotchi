-- Operator control is separate from enrollment identity. A resume cannot restore old admissions.
ALTER TABLE machine_devices ADD COLUMN access_paused BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE machine_devices ADD COLUMN gui_paused BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE machine_devices ADD COLUMN access_version BIGINT NOT NULL DEFAULT 1 CHECK(access_version>0);
