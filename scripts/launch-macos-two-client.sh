#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: $0 /path/to/Links.app [profile-root] [alice-profile] [bob-profile]" >&2
  exit 2
}

if [[ $# -lt 1 || $# -gt 4 ]]; then
  usage
fi

app_path="$1"
profile_root="${2:-${LINKS_PROFILE_ROOT:-$HOME/Library/Application Support/Links/profiles}}"
alice_profile="${3:-alice}"
bob_profile="${4:-bob}"
auth_url="${LINKS_AUTH_URL:-http://127.0.0.1:8080}"

if [[ ! -d "$app_path" ]]; then
  echo "App not found: $app_path" >&2
  exit 1
fi

if [[ "$alice_profile" == "$bob_profile" ]]; then
  echo "Profiles must be different: $alice_profile" >&2
  exit 1
fi

mkdir -p "$profile_root"

open -n "$app_path" --args \
  --profile "$alice_profile" \
  --profile-root "$profile_root" \
  --auth-url "$auth_url"
open -n "$app_path" --args \
  --profile "$bob_profile" \
  --profile-root "$profile_root" \
  --auth-url "$auth_url"

echo "Started $alice_profile and $bob_profile"
echo "Wait for: $profile_root/$alice_profile/status.json"
echo "Wait for: $profile_root/$bob_profile/status.json"
