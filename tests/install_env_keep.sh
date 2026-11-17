#!/usr/bin/env bash
# install.sh keeps tenancy, operators and sign-in providers when a re-run omits those flags.
# The pure half of that rule, with no Docker and no root. The deploy-script test re-runs
# install.sh itself against a hub.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=../deploy/compose/env.sh
. "$root/deploy/compose/env.sh"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
cat > "$dir/.env" <<'EOF'
DEVAGOTCHI_HUB_TENANCY=multi
DEVAGOTCHI_HUB_OPERATORS=email:ops@test.example
DEVAGOTCHI_HUB_PROVIDERS='[{"kind":"oidc","name":"nomen","label":"Shippin","issuer":"https://id.example.com","client_id_env":"DEVAGOTCHI_NOMEN_CLIENT_ID"}]'
DEVAGOTCHI_HUB_EMAIL='null'
DEVAGOTCHI_HUB_ANALYTICS=on
DEVAGOTCHI_HUB_ANALYTICS_RETENTION_DAYS=30
EOF

[[ "$(keep_env DEVAGOTCHI_HUB_TENANCY "" false "$dir/.env")" == multi ]]
[[ "$(keep_env DEVAGOTCHI_HUB_TENANCY single true "$dir/.env")" == single ]]
[[ "$(keep_env DEVAGOTCHI_HUB_OPERATORS "" false "$dir/.env")" == email:ops@test.example ]]
[[ "$(keep_env DEVAGOTCHI_HUB_OPERATORS "" true "$dir/.env")" == "" ]]
providers="$(keep_env DEVAGOTCHI_HUB_PROVIDERS '[]' false "$dir/.env")"
[[ "$providers" == *'"name":"nomen"'* ]]
[[ "$(keep_env DEVAGOTCHI_HUB_ANALYTICS_RETENTION_DAYS "" false "$dir/.env")" == 30 ]]
[[ "$(keep_env DEVAGOTCHI_HUB_EMAIL null false "$dir/.env")" == null ]]
[[ "$(keep_env DEVAGOTCHI_HUB_TENANCY "" false "$dir/missing.env")" == "" ]]
echo "PASS install.sh keeps omitted tenancy, operators and Shippin settings"
