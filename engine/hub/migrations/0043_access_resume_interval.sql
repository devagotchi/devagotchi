-- Add the source resume interval explicitly to saved settings and invalidate stale edits.
UPDATE devagotchi.machine_access_settings
SET value=jsonb_set(value,'{version}',to_jsonb(version+1)) || jsonb_build_object('resume_alert_idle_minutes',15),
    version=version+1
WHERE NOT jsonb_exists(value,'resume_alert_idle_minutes');
