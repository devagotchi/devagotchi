#!/usr/bin/env bash
# A throwaway ClickHouse for a CI job on Linux: the image the compose stack pins, as the
# container dvg-ch on 127.0.0.1:18123, user devagotchi with a password made here and
# handed to the job's later steps through $GITHUB_ENV (masked) or printed for a local run.
# The hub exports its board events there (crates/agenthub/src/clickhouse.rs):
#   tests/throwaway_clickhouse.sh
#   DEVAGOTCHI_CLICKHOUSE_URL=http://127.0.0.1:18123 DEVAGOTCHI_CLICKHOUSE_USER=devagotchi \
#   DEVAGOTCHI_CLICKHOUSE_PASSWORD=... devagotchi hub serve …
set -euo pipefail
image=$(sed -n 's/.*DEVAGOTCHI_CLICKHOUSE_IMAGE:-\([^}]*\)}.*/\1/p' "$(dirname "$0")/../deploy/compose/compose.yaml")
[[ -n "$image" ]] || { echo "no ClickHouse image pinned in deploy/compose/compose.yaml" >&2; exit 1; }
password=$(od -An -N24 -tx1 /dev/urandom | tr -d ' \n')
docker rm -f dvg-ch >/dev/null 2>&1 || true
docker run -d --name dvg-ch -p 127.0.0.1:18123:8123 --ulimit nofile=262144:262144 \
  -e CLICKHOUSE_USER=devagotchi -e CLICKHOUSE_PASSWORD="$password" \
  -e CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT=0 "$image" >/dev/null
for _ in $(seq 1 60); do
  curl -fsS -o /dev/null http://127.0.0.1:18123/ping 2>/dev/null && break
  sleep 1
done
curl -fsS -o /dev/null http://127.0.0.1:18123/ping || { docker logs dvg-ch >&2; exit 1; }
if [[ -n "${GITHUB_ENV:-}" ]]; then
  echo "::add-mask::$password"
  {
    echo "DEVAGOTCHI_CLICKHOUSE_PASSWORD=$password"
    echo "DEVAGOTCHI_TEST_CLICKHOUSE_PASSWORD=$password"
  } >> "$GITHUB_ENV"
else
  echo "DEVAGOTCHI_CLICKHOUSE_PASSWORD=$password"
fi
echo "ClickHouse ($image) is up as dvg-ch on http://127.0.0.1:18123, user devagotchi"
