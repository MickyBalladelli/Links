#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

auth_url="${SMOKE_AUTH_URL:-${LINKS_AUTH_URL:-http://127.0.0.1:8080}}"
gateway_url="${SMOKE_GATEWAY_URL:-${LINKS_GATEWAY_ENDPOINT:-ws://127.0.0.1:8081/v1/connect}}"

SMOKE_AUTH_URL="$auth_url" \
SMOKE_GATEWAY_URL="$gateway_url" \
cargo run --quiet -p links-gateway --bin links-two-client-smoke --locked
