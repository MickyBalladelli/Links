#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

usage() {
  echo "Usage: $0 shared|gateway|all" >&2
  exit 2
}

suite="${1:-all}"

run_shared() {
  # Shared client behavior. The desktop FFI tests include desktop <-> browser
  # encrypted text, replies, simultaneous sends, and browser reload.
  cargo test -p links-client-core --locked
  cargo test -p links-desktop-client-ffi --locked
}

run_gateway() {
  # Uses the already-running local auth service and gateway. It creates new
  # new test username accounts for every run.
  ./scripts/smoke-two-client.sh
}

case "$suite" in
  shared)
    run_shared
    ;;
  gateway)
    run_gateway
    ;;
  all)
    run_shared
    run_gateway
    ;;
  *)
    usage
    ;;
esac
