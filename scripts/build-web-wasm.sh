#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
wasm_target="$repo_root/target/wasm32-unknown-unknown/release/links_web_client.wasm"
output_dir="$repo_root/web/client/public/web-wasm"

cargo build \
  --manifest-path "$repo_root/Cargo.toml" \
  -p links-web-client \
  --target wasm32-unknown-unknown \
  --release \
  --locked

mkdir -p "$output_dir"
if command -v wasm-bindgen >/dev/null 2>&1; then
  wasm-bindgen "$wasm_target" --target web --out-dir "$output_dir"
elif command -v wasm-pack >/dev/null 2>&1; then
  wasm-pack build "$repo_root/crates/web-client" --target web --out-dir "$output_dir"
else
  echo "Install wasm-bindgen-cli 0.2.128 or wasm-pack before building the Web client." >&2
  exit 1
fi
