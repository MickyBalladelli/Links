#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$repo_root"

if [[ $# -ne 0 ]]; then
  echo "Usage: ./cleanup.sh" >&2
  exit 2
fi

# Regenerable build output. .env and native/ios/LocalHTTPS stay: the first
# holds local secrets, the second is the iPhone trust certificate.
paths=(
  Trash
  target
  web/pkg
  web/http-client/node_modules
  web/http-client/dist
  .local-tools
  node_modules
  native/apple/.build
  native/apple/.swiftpm
  native/apple/build
  native/android/.gradle
  native/android/build
  native/android/local.properties
  native/macos/.build
  native/macos/.swiftpm
  native/macos/DerivedData
  native/macos/DerivedDataAdmin
  native/macos/build
  native/ios/DerivedData
  native/ios/build
  .xcode-derived-data
  native/macos/Links.xcodeproj/xcuserdata
  native/macos/Links.xcodeproj/project.xcworkspace/xcuserdata
)

removed=0
for path in "${paths[@]}"; do
  if [[ -e "$path" || -L "$path" ]]; then
    echo "Removing $path"
    rm -rf -- "$path"
    removed=$((removed + 1))
  fi
done

while IFS= read -r -d '' file; do
  echo "Removing ${file#./}"
  rm -f -- "$file"
  removed=$((removed + 1))
done < <(find . \
  \( -name .git -o -name LocalHTTPS \) -prune -o \
  \( -name .DS_Store -o -name '*.log' -o -name '*.xcuserstate' \) -type f -print0)

if [[ "$removed" -eq 0 ]]; then
  echo "Nothing to remove."
else
  echo "Removed $removed item(s)."
fi
