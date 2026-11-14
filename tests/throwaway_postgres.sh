#!/usr/bin/env bash
# A throwaway PostgreSQL for a CI job on Linux: the runner image's own server binaries, a
# cluster of its own on 127.0.0.1:55432 (trust authentication, user postgres), and one empty
# database per name given. A hub keeps its accounts and boards in Postgres (docs/adr/0001)
# and a database holds one hub (crates/store/migrations/postgres/0007_tenant_rls.sql), so
# every hub a job starts gets a database of its own, named here:
#   tests/throwaway_postgres.sh e2e claim passkey
#   DEVAGOTCHI_HUB_STORAGE_URL=postgres://postgres@127.0.0.1:55432/e2e python tests/e2e.py …
set -euo pipefail
if ! ls /usr/lib/postgresql/*/bin/initdb >/dev/null 2>&1; then
  sudo apt-get update -q && sudo apt-get install -yq postgresql
fi
bin=$(ls -d /usr/lib/postgresql/*/bin | sort -V | tail -1)
sudo install -d -o postgres -m 700 /var/tmp/dvg-pg
sudo -u postgres "$bin/initdb" -D /var/tmp/dvg-pg/data -U postgres --auth=trust -E UTF8 >/dev/null
sudo -u postgres "$bin/pg_ctl" -D /var/tmp/dvg-pg/data -l /var/tmp/dvg-pg/log -w \
  -o "-k /var/tmp/dvg-pg -p 55432 -c listen_addresses=127.0.0.1" start
for db in "$@"; do
  "$bin/createdb" -h 127.0.0.1 -p 55432 -U postgres "$db"
done
echo "postgres://postgres@127.0.0.1:55432 is up; databases: ${*:-(none, the default 'postgres' only)}"
