#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$repo_root"

derived_data_path="${LINKS_ADMIN_DERIVED_DATA_PATH:-$repo_root/native/macos/DerivedDataAdmin}"
app_path="${LINKS_ADMIN_APP_PATH:-$derived_data_path/Build/Products/Debug/links-admin.app}"

if [[ "${LINKS_ADMIN_BUILD_APP:-1}" == "1" ]]; then
  xcodebuild \
    -project native/macos/Links.xcodeproj \
    -scheme links-admin \
    -configuration Debug \
    -sdk macosx \
    -derivedDataPath "$derived_data_path" \
    -allowProvisioningUpdates \
    build
fi

if [[ ! -d "$app_path" ]]; then
  echo "Admin app not found: $app_path" >&2
  echo "Set LINKS_ADMIN_APP_PATH to a built links-admin.app or leave LINKS_ADMIN_BUILD_APP=1." >&2
  exit 1
fi

open -n "$app_path"
echo "Started links-admin"
