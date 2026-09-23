#!/bin/sh
# MWM - Move Weight Manager: installer for Linux servers (Proxmox VE, Unraid,
# Debian/Ubuntu, anything x86_64). Static binary - no dependencies.
#
#   curl -fsSL https://github.com/BlizzHacker/mwm/releases/latest/download/install.sh | sh
#   curl -fsSL .../install.sh | sh -s -- --web              # + browser UI on :7777
#   curl -fsSL .../install.sh | sh -s -- --web --read-only  # look, don't touch
#   curl -fsSL .../install.sh | sh -s -- --uninstall
set -eu

REPO="BlizzHacker/mwm"
BASE="https://github.com/$REPO/releases/latest/download"
BIN=/usr/local/bin/mwm
WEB=0
BIND="0.0.0.0:7777"
RO=""
UNINSTALL=0

while [ $# -gt 0 ]; do
  case "$1" in
    --web) WEB=1 ;;
    --bind) BIND="$2"; shift ;;
    --read-only) RO="--read-only" ;;
    --uninstall) UNINSTALL=1 ;;
    *) echo "unknown option $1"; exit 2 ;;
  esac
  shift
done

say() { printf '\033[1;32m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31mxx\033[0m %s\n' "$*" >&2; exit 1; }

[ "$(id -u)" = 0 ] || die "run as root (sudo sh, or as root on Proxmox/Unraid)"
HAS_SYSTEMD=0; command -v systemctl >/dev/null 2>&1 && [ -d /run/systemd/system ] && HAS_SYSTEMD=1

if [ "$UNINSTALL" = 1 ]; then
  if [ "$HAS_SYSTEMD" = 1 ]; then systemctl disable --now mwm-web >/dev/null 2>&1 || true; rm -f /etc/systemd/system/mwm-web.service; systemctl daemon-reload; fi
  pkill -x mwm 2>/dev/null || true
  rm -f "$BIN"
  say "MWM removed (data in /var/lib/MWM kept)"
  exit 0
fi

case "$(uname -m)" in x86_64|amd64) ;; *) die "only x86_64 builds are published so far" ;; esac
fetch() { if command -v curl >/dev/null 2>&1; then curl -fsSL "$1" -o "$2"; else wget -qO "$2" "$1"; fi; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
say "Downloading mwm"
fetch "$BASE/mwm-linux-x86_64" "$tmp/mwm"
fetch "$BASE/SHA256SUMS" "$tmp/SHA256SUMS"
want=$(grep ' mwm-linux-x86_64$' "$tmp/SHA256SUMS" | cut -d' ' -f1)
got=$(sha256sum "$tmp/mwm" | cut -d' ' -f1)
[ -n "$want" ] && [ "$want" = "$got" ] || die "checksum mismatch - refusing to install"
install -m 0755 "$tmp/mwm" "$BIN"
say "Installed $($BIN --version) to $BIN"

if [ -f /etc/unraid-version ]; then
  say "Unraid detected: /usr/local/bin is in RAM - install the plugin for persistence:"
  echo "    Plugins > Install Plugin > $BASE/mwm.plg"
fi

if [ "$WEB" = 1 ]; then
  if [ "$HAS_SYSTEMD" = 1 ]; then
    cat > /etc/systemd/system/mwm-web.service <<EOF
[Unit]
Description=MWM - Move Weight Manager web UI
After=network-online.target

[Service]
Environment=XDG_DATA_HOME=/var/lib
ExecStart=$BIN serve --bind $BIND $RO
Restart=on-failure

[Install]
WantedBy=multi-user.target
EOF
    systemctl daemon-reload
    systemctl enable --now mwm-web >/dev/null
  else
    mkdir -p /var/lib
    XDG_DATA_HOME=/var/lib nohup "$BIN" serve --bind "$BIND" $RO >/var/log/mwm.log 2>&1 &
  fi
  sleep 1
  TOKEN=$(XDG_DATA_HOME=/var/lib "$BIN" serve --show-token)
  HOST=$(hostname -I 2>/dev/null | awk '{print $1}')
  say "Web UI: http://${HOST:-$(hostname)}:${BIND##*:}/?token=$TOKEN"
  echo "    (plain HTTP - keep it on your LAN or behind a reverse proxy / VPN)"
fi

say "Try:  mwm info   |   mwm scan   |   mwm server   |   mwm keys   |   mwm repair --list"
