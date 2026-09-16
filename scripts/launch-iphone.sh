#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

team="${DEVELOPMENT_TEAM:-${LINKS_DEVELOPMENT_TEAM:-3SZ568CM7P}}"
auth_url="${LINKS_AUTH_URL:-}"
device_id="${LINKS_IOS_DEVICE_ID:-00008030-001D44593E6B402E}"
derived_data_path="${LINKS_IOS_DERIVED_DATA_PATH:-$repo_root/native/ios/DerivedData}"
proxy_started=0

proxy_health() {
  local health_response
  health_response="$(curl --silent --show-error --fail --insecure --max-time 1 "$auth_url/healthz" 2>/dev/null || true)"
  [[ "$health_response" == *'"proxy":"links-https-v3"'* ]] && [[ -f "$root_cert" ]]
}

if [[ -n "$auth_url" ]]; then
  authority="$(printf '%s' "$auth_url" | sed -E 's#^[^:]+://([^/]+).*$#\1#')"
  case "$authority" in
    127.0.0.1|127.0.0.1:*|localhost|localhost:*|::1|::1:*)
      echo "Loopback auth URL cannot work on a physical iPhone: $auth_url" >&2
      echo "Using the Mac LAN endpoint instead." >&2
      auth_url=""
      ;;
  esac
fi

if [[ -z "$team" ]]; then
  echo "Set DEVELOPMENT_TEAM to your Apple Development Team ID." >&2
  echo "Example: LINKS_AUTH_URL=https://auth.example bash scripts/launch-iphone.sh" >&2
  exit 1
fi

if [[ -z "$auth_url" ]]; then
  default_interface="$(route -n get default 2>/dev/null | awk '/interface:/{print $2; exit}')"
  lan_ip=""
  if [[ -n "$default_interface" ]]; then
    lan_ip="$(ipconfig getifaddr "$default_interface" 2>/dev/null || true)"
  fi
  if [[ -z "$lan_ip" ]]; then
    for interface in en0 en1; do
      lan_ip="$(ipconfig getifaddr "$interface" 2>/dev/null || true)"
      if [[ -n "$lan_ip" ]]; then
        break
      fi
    done
  fi
  if [[ -z "$lan_ip" ]]; then
    echo "Could not determine the Mac LAN IP. Set LINKS_AUTH_URL manually." >&2
    exit 1
  fi
  auth_port="${LINKS_AUTH_PORT:-8443}"
  auth_url="https://${lan_ip}:${auth_port}"
  echo "Using Mac LAN auth endpoint: $auth_url" >&2

  proxy_dir="${LINKS_HTTPS_PROXY_DIR:-$repo_root/native/ios/LocalHTTPS}"
  proxy_log="$proxy_dir/proxy.log"
  mkdir -p "$proxy_dir"
  root_cert="$proxy_dir/root-cert.cer"
  if ! proxy_health; then
    if [[ "${LINKS_START_HTTPS_PROXY:-1}" != "1" ]]; then
      echo "No HTTPS proxy is listening at $auth_url." >&2
      echo "Start it with: LINKS_LAN_IP=$lan_ip LINKS_HTTPS_PORT=$auth_port node scripts/local-https-proxy.mjs" >&2
      exit 1
    fi

    stale_pid="$(lsof -tiTCP:"$auth_port" -sTCP:LISTEN 2>/dev/null | head -n1 || true)"
    if [[ -n "$stale_pid" ]]; then
      echo "Stopping stale HTTPS proxy on port $auth_port." >&2
      kill "$stale_pid" 2>/dev/null || true
      for _ in {1..20}; do
        if ! kill -0 "$stale_pid" 2>/dev/null; then
          break
        fi
        sleep 0.25
      done
    fi

    echo "Starting local HTTPS proxy; log: $proxy_log" >&2
    LINKS_LAN_IP="$lan_ip" \
    LINKS_HTTPS_PORT="$auth_port" \
    node "$repo_root/scripts/local-https-proxy.mjs" >>"$proxy_log" 2>&1 &
    proxy_pid=$!
    for _ in {1..40}; do
      if proxy_health; then
        proxy_started=1
        break
      fi
      if ! kill -0 "$proxy_pid" 2>/dev/null; then
        break
      fi
      sleep 0.25
    done
    if [[ "$proxy_started" != "1" ]]; then
      echo "HTTPS proxy did not become ready. Read $proxy_log" >&2
      exit 1
    fi
    echo "HTTPS proxy ready on $auth_url" >&2
  fi
  echo "Install this iPhone trust profile: $proxy_dir/root-cert.mobileconfig" >&2
  echo "Certificate fallback: $root_cert" >&2
fi

case "$auth_url" in
  https://*)
    ;;
  *)
    echo "LINKS_AUTH_URL must use HTTPS for iPhone OTP." >&2
    exit 1
    ;;
esac

if [[ -z "$device_id" ]]; then
  echo "Set LINKS_IOS_DEVICE_ID to the connected iPhone UDID." >&2
  echo "Find it with: xcrun xcdevice list" >&2
  exit 1
fi

DEVELOPMENT_TEAM="$team" \
LINKS_AUTH_URL="$auth_url" \
LINKS_IOS_DESTINATION="id=$device_id" \
bash "$repo_root/scripts/build-ios-debug.sh"

xcrun devicectl device install app \
  --device "$device_id" \
  "$derived_data_path/Build/Products/Debug-iphoneos/Links.app"

echo "Installed Links on iPhone: $device_id"

if [[ "$proxy_started" == "1" ]]; then
  echo "The HTTPS proxy remains running for the iPhone. Stop it with: pkill -f scripts/local-https-proxy.mjs" >&2
fi
