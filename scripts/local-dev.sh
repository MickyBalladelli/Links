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

local_auth_probe="http://${AUTH_BIND}/v1/auth/me"
if curl -sS --max-time 1 -o /dev/null "$local_auth_probe"; then
  local_admin_delete_probe="http://${AUTH_BIND}/v1/admin/users/00000000-0000-0000-0000-000000000000"
  local_admin_delete_status="$(curl -sS --max-time 1 -o /dev/null -w '%{http_code}' -X DELETE "$local_admin_delete_probe")"
  if [[ "$local_admin_delete_status" == "404" ]]; then
    echo "Local backend is running an older build without the admin delete API. Stop it, then run this script again." >&2
    exit 1
  fi
  echo "Local backend already running at $local_auth_probe"
  exit 0
fi

postgres_mode="${LINKS_USE_DOCKER:-auto}"
if [[ "$postgres_mode" != "auto" && "$postgres_mode" != "0" && "$postgres_mode" != "1" ]]; then
  echo "LINKS_USE_DOCKER must be auto, 0, or 1." >&2
  exit 1
fi

docker_available=0
if command -v docker >/dev/null 2>&1; then
  docker_available=1
fi

if [[ "$postgres_mode" == "1" || ( "$postgres_mode" == "auto" && "$docker_available" == "1" ) ]]; then
  : "${DATABASE_URL:=postgresql://links:links-local-only@127.0.0.1:5432/links}"
  export DATABASE_URL
  docker compose up -d --wait postgres
else
  if [[ "$postgres_mode" == "1" ]]; then
    echo "Docker was requested but is not installed." >&2
    exit 1
  fi

  echo "Docker not found; using host PostgreSQL from DATABASE_URL." >&2
  : "${DATABASE_URL:?Set DATABASE_URL in .env for host PostgreSQL}"

  pg_isready_path="$(command -v pg_isready || true)"
  if [[ -z "$pg_isready_path" ]]; then
    for candidate in /Applications/Postgres.app/Contents/Versions/*/bin/pg_isready; do
      if [[ -x "$candidate" ]]; then
        pg_isready_path="$candidate"
        break
      fi
    done
  fi

  if [[ -n "$pg_isready_path" ]] && ! "$pg_isready_path" -d "$DATABASE_URL" >/dev/null 2>&1; then
    echo "Host PostgreSQL is not ready for the configured database." >&2
    exit 1
  fi
fi

cargo run -p links-gateway --bin links-local-dev --locked
