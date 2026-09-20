#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
client_root="$repo_root/web/client"

if [[ ! -d "$client_root/node_modules" ]]; then
  npm install --prefix "$client_root"
fi

exec npm run dev --prefix "$client_root"
