#!/usr/bin/env bash
# Update a running MWM Linux service from its published stable GitHub release.
# A checksum is mandatory; the previous binary is kept for rollback.
set -euo pipefail

service="${1:-}"
case "$service" in mwm-web|mwm-agent) ;; *) echo "usage: mwm-self-update {mwm-web|mwm-agent}" >&2; exit 2 ;; esac
test "$(id -u)" -eq 0 || { echo "root required" >&2; exit 2; }
command -v curl >/dev/null
command -v sha256sum >/dev/null
command -v flock >/dev/null
command -v sort >/dev/null
exec 9>/run/lock/mwm-self-update.lock
flock -n 9 || { echo "MWM update already running"; exit 0; }

bin=/usr/local/bin/mwm
state="${MWM_UPDATE_DIR:-/var/lib/mwm/updates}"
mkdir -p "$state"
chmod 700 "$state"
test -x "$bin"
systemctl is-active --quiet "$service" || { echo "$service is not active" >&2; exit 1; }

latest_url=$(curl --fail --silent --show-error --location --output /dev/null --write-out '%{url_effective}' --max-time 30 https://github.com/BlizzHacker/mwm/releases/latest)
tag="${latest_url##*/}"
[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "Invalid release tag: $tag" >&2; exit 1; }
current=$("$bin" --version | awk '{print $2}')
[[ "$current" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "Invalid installed version: $current" >&2; exit 1; }
if [[ "v$current" == "$tag" ]]; then
  echo "MWM $current is current"
  exit 0
fi
if [[ "$(printf '%s\n%s\n' "$current" "${tag#v}" | sort -V | tail -n 1)" != "${tag#v}" ]]; then
  echo "Skipping older release $tag; installed $current"
  exit 0
fi

tmp=$(mktemp -d "$state/stage.XXXXXXXX")
trap 'rm -rf "$tmp"' EXIT
base="https://github.com/BlizzHacker/mwm/releases/download/$tag"
curl --fail --silent --show-error --location --retry 3 --max-time 120 "$base/mwm-linux-x86_64" -o "$tmp/mwm-linux-x86_64"
curl --fail --silent --show-error --location --retry 3 --max-time 30 "$base/SHA256SUMS" -o "$tmp/SHA256SUMS"
(
  cd "$tmp"
  grep -E '^[[:xdigit:]]{64}  mwm-linux-x86_64$' SHA256SUMS | head -n 1 | sha256sum -c -
)
chmod 755 "$tmp/mwm-linux-x86_64"
test "$("$tmp/mwm-linux-x86_64" --version)" = "mwm ${tag#v}" || { echo "Downloaded binary version mismatch" >&2; exit 1; }

cp -p "$bin" "$state/mwm.previous"
install -m 0755 "$tmp/mwm-linux-x86_64" "$state/mwm.next"
mv -f "$state/mwm.next" "$bin"
if systemctl restart "$service" && sleep 3 && systemctl is-active --quiet "$service" && test "$("$bin" --version)" = "mwm ${tag#v}"; then
  printf '%s\t%s\t%s\n' "$(date -u +%FT%TZ)" "$current" "${tag#v}" > "$state/last-success"
  echo "Updated $service from $current to ${tag#v}"
else
  install -m 0755 "$state/mwm.previous" "$bin"
  systemctl restart "$service" || true
  echo "Update failed; restored MWM $current" >&2
  exit 1
fi
