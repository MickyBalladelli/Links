#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
client_root="$repo_root/web/client"

if [[ -f "$repo_root/.env" ]]; then
  set -a
  . "$repo_root/.env"
  set +a
fi

if [[ ! -d "$client_root/node_modules" ]]; then
  npm install --prefix "$client_root"
fi

if [[ ! -f "$client_root/public/web-wasm/links_web_client.js" ]]; then
  npm run build:wasm --prefix "$repo_root/web"
fi

exec npm run dev --prefix "$client_root"
