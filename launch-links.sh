#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$repo_root"

if [[ $# -ne 0 ]]; then
  echo "Usage: ./launch-links.sh" >&2
  exit 2
fi

if [[ ! -f .env ]]; then
  echo "Create .env from .env.example first." >&2
  exit 1
fi

set -a
. ./.env
set +a

if [[ -z "${AUTH_LOOKUP_KEY:-}" || ! "$AUTH_LOOKUP_KEY" =~ ^[A-Za-z0-9_-]{43}$ ]]; then
  echo "AUTH_LOOKUP_KEY is invalid in .env." >&2
  echo "Generate one with: openssl rand -base64 32 | tr '+/' '-_' | tr -d '='" >&2
  exit 1
fi

derived_data_path="${LINKS_DERIVED_DATA_PATH:-$repo_root/native/macos/DerivedData}"
app_path="${LINKS_APP_PATH:-$derived_data_path/Build/Products/Debug/Links.app}"
profile_root="${LINKS_PROFILE_ROOT:-}"
auth_url="${LINKS_AUTH_URL:-http://127.0.0.1:8080}"

if [[ "${LINKS_BUILD_APP:-1}" == "1" ]]; then
  xcodebuild_args=(
    -project native/macos/Links.xcodeproj
    -scheme Links-Debug
    -configuration Debug
    -sdk macosx
    -derivedDataPath "$derived_data_path"
  )
  if [[ -n "${LINKS_DEVELOPMENT_TEAM:-}" ]]; then
    xcodebuild_args+=("DEVELOPMENT_TEAM=$LINKS_DEVELOPMENT_TEAM")
  fi
  if [[ "${LINKS_ALLOW_PROVISIONING_UPDATES:-1}" == "1" ]]; then
    xcodebuild_args+=(-allowProvisioningUpdates)
  fi

  MACOSX_DEPLOYMENT_TARGET=13.0 cargo build -p links-identity-ffi --locked
  MACOSX_DEPLOYMENT_TARGET=13.0 xcodebuild "${xcodebuild_args[@]}" build
fi

if [[ ! -d "$app_path" ]]; then
  echo "App not found: $app_path" >&2
  echo "Set LINKS_APP_PATH to a built Links.app or leave LINKS_BUILD_APP=1." >&2
  exit 1
fi

backend_pid=""
cleanup() {
  if [[ -n "$backend_pid" ]] && kill -0 "$backend_pid" 2>/dev/null; then
    kill "$backend_pid" 2>/dev/null || true
    wait "$backend_pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

if curl -sS --max-time 1 -o /dev/null "$auth_url/v1/auth/me"; then
  echo "Using running local backend at $auth_url"
else
  echo "Starting local backend with host PostgreSQL"
  LINKS_USE_DOCKER=0 bash "$repo_root/scripts/local-dev.sh" &
  backend_pid="$!"

  ready=0
  for _ in {1..60}; do
    if curl -sS --max-time 1 -o /dev/null "$auth_url/v1/auth/me"; then
      ready=1
      break
    fi
    if ! kill -0 "$backend_pid" 2>/dev/null; then
      wait "$backend_pid" 2>/dev/null || true
      break
    fi
    sleep 1
  done

  if [[ "$ready" != "1" ]]; then
    echo "Local backend did not become ready at $auth_url" >&2
    exit 1
  fi
fi

if [[ -n "$profile_root" ]]; then
  LINKS_AUTH_URL="$auth_url" \
    bash "$repo_root/scripts/launch-macos-two-client.sh" \
    "$app_path" "$profile_root" "${LINKS_ALICE_PROFILE:-alice}" "${LINKS_BOB_PROFILE:-bob}"
else
  LINKS_AUTH_URL="$auth_url" \
    bash "$repo_root/scripts/launch-macos-two-client.sh" \
    "$app_path" "" "${LINKS_ALICE_PROFILE:-alice}" "${LINKS_BOB_PROFILE:-bob}"
fi

if [[ -n "$backend_pid" ]]; then
  wait "$backend_pid"
fi
