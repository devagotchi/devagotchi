#!/usr/bin/env bash
# Back up the hub: its database (pg_dump, custom format) and its state volume (keys,
# machine enrolments, audit), kept for DEVAGOTCHI_BACKUP_DAYS (14). install.sh's timer runs
# it nightly. Copy the directory off this host too.
#
# Restore both, on a stack install.sh made (on a new server, run install.sh first):
#   sudo ./backup.sh --restore DB.dump STATE.tgz
# It replaces the database and the state volume with the backup's; anything since is lost.
# ClickHouse is not backed up here: it holds what Postgres's board history already holds.
# After a restore, or with an empty ClickHouse, `docker compose exec hub devagotchi hub
# analytics-resend --config /etc/devagotchi/hub.json` sends every event again; ClickHouse
# keeps one row per event.
set -euo pipefail
dir="${DEVAGOTCHI_DIR:-/opt/devagotchi-hub}"
out="${DEVAGOTCHI_BACKUP_DIR:-/var/backups/devagotchi-hub}"
days="${DEVAGOTCHI_BACKUP_DAYS:-14}"
cd "$dir"

# The hub-state volume, and an image with tar in it (the hub's own has no shell).
locate_state() {
  pg=$(docker compose ps -q postgres)
  project=$(docker inspect --format '{{index .Config.Labels "com.docker.compose.project"}}' "$pg")
  volume=$(docker volume ls -q --filter "label=com.docker.compose.project=$project" --filter "label=com.docker.compose.volume=hub-state")
  [[ -n "$volume" ]] || { echo "devagotchi-hub-backup: no hub-state volume for project $project" >&2; exit 1; }
  image=$(docker inspect --format '{{.Image}}' "$pg")
}

if [[ "${1:-}" == --restore ]]; then
  db="${2:-}" archive="${3:-}"
  [[ -f "$db" && -f "$archive" ]] || { echo "usage: $0 --restore DB.dump STATE.tgz" >&2; exit 2; }
  # Nothing deploys while it runs. If a step fails the timer stays stopped, so no update lands
  # on a half-restored hub.
  systemctl stop devagotchi-hub-update.timer 2>/dev/null || true
  docker compose stop hub
  docker compose up -d --wait --wait-timeout 120 postgres
  docker compose exec -T postgres psql -v ON_ERROR_STOP=1 -q -h /var/run/postgresql -U devagotchi -d devagotchi \
    -c 'DROP SCHEMA IF EXISTS devagotchi CASCADE'
  docker compose exec -T postgres pg_restore --exit-on-error --clean --if-exists \
    -h /var/run/postgresql -U devagotchi -d devagotchi < "$db"
  locate_state
  docker run --rm -i --network none -v "$volume:/state" "$image" \
    sh -c 'find /state -mindepth 1 -delete && tar -C /state -xzf -' < "$archive"
  docker compose up -d --wait --wait-timeout 180 hub
  systemctl start devagotchi-hub-update.timer 2>/dev/null || true
  echo "devagotchi-hub-backup: restored $db and $archive"
  exit 0
fi

install -d -m 0700 "$out"
ts=$(date -u +%Y%m%dT%H%M%SZ)
docker compose exec -T postgres pg_dump -h /var/run/postgresql -U devagotchi --format=custom devagotchi \
  > "$out/db-$ts.dump.part"
mv "$out/db-$ts.dump.part" "$out/db-$ts.dump"
locate_state
docker run --rm --network none -v "$volume:/state:ro" "$image" tar -C /state -czf - . > "$out/state-$ts.tgz.part"
mv "$out/state-$ts.tgz.part" "$out/state-$ts.tgz"
find "$out" -maxdepth 1 \( -name 'db-*.dump' -o -name 'state-*.tgz' \) -mtime +"$days" -delete
echo "devagotchi-hub-backup: $out/db-$ts.dump, $out/state-$ts.tgz"
