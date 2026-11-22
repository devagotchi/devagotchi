-- A new enrollment is a new authority even if name, key and second are unchanged.
-- Hosted registry remains global for link bootstrap; owner checks stay in its owner.
ALTER TABLE machine_devices ADD COLUMN generation TEXT NOT NULL
    DEFAULT ('g-' || replace(gen_random_uuid()::text, '-', ''));
CREATE UNIQUE INDEX machine_device_generation ON machine_devices(generation);
