#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [[ ! -f .env ]]; then
  echo "Create .env from .env.example and set AUTH_LOOKUP_KEY first." >&2
  exit 1
fi

set -a
. ./.env
set +a

: "${AUTH_LOOKUP_KEY:?Set AUTH_LOOKUP_KEY in .env}"
export AUTH_DEV_USERNAME_MODE=1
export AUTH_BIND="${AUTH_BIND:-127.0.0.1:8080}"
export GATEWAY_BIND="${GATEWAY_BIND:-127.0.0.1:8081}"
export LINKS_AUTH_URL="${LINKS_AUTH_URL:-http://127.0.0.1:8080}"
export LINKS_GATEWAY_ENDPOINT="${LINKS_GATEWAY_ENDPOINT:-ws://127.0.0.1:8081/v1/connect}"

docker compose up -d --wait postgres
cargo run -p links-gateway --bin links-local-dev --locked
