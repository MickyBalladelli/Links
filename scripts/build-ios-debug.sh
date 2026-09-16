#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

team="${DEVELOPMENT_TEAM:-${LINKS_DEVELOPMENT_TEAM:-}}"
if [[ -z "$team" ]]; then
  echo "Set DEVELOPMENT_TEAM to your Apple Development Team ID." >&2
  echo "Example: DEVELOPMENT_TEAM=ABCDE12345 bash scripts/build-ios-debug.sh" >&2
  exit 1
fi

if ! rustup target list --installed | rg -q '^aarch64-apple-ios$'; then
  echo "Missing Rust target: aarch64-apple-ios" >&2
  echo "Install it with: rustup target add aarch64-apple-ios" >&2
  exit 1
fi

MACOSX_DEPLOYMENT_TARGET=13.0 cargo build \
  --target aarch64-apple-ios \
  -p links-identity-ffi \
  -p links-desktop-client-ffi \
  --locked

derived_data_path="${LINKS_IOS_DERIVED_DATA_PATH:-$repo_root/native/ios/DerivedData}"
auth_url="${LINKS_AUTH_URL:-https://api.links.invalid}"
provisioning_args=()
if [[ "${LINKS_ALLOW_PROVISIONING_UPDATES:-1}" == "1" ]]; then
  provisioning_args+=(-allowProvisioningUpdates)
  provisioning_args+=(-allowProvisioningDeviceRegistration)
fi

LINKS_IDENTITY_LIB_DIR="$repo_root/target/aarch64-apple-ios/debug" \
  xcodebuild \
    -project native/ios/LinksIOS.xcodeproj \
    -scheme Links-iOS-Debug \
    -configuration Debug \
    -sdk iphoneos \
    -destination 'generic/platform=iOS' \
    -derivedDataPath "$derived_data_path" \
    DEVELOPMENT_TEAM="$team" \
    LINKS_AUTH_URL="$auth_url" \
    CODE_SIGN_STYLE=Automatic \
    "${provisioning_args[@]}" \
    build

echo "Built: $derived_data_path/Build/Products/Debug-iphoneos/Links.app"
