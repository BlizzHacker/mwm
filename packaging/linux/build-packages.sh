#!/bin/sh
# Build the Linux release artifacts from the static musl binary:
#   mwm-linux-x86_64, mwm_<ver>_amd64.deb, mwm.plg (Unraid), install.sh, SHA256SUMS
# Usage: packaging/linux/build-packages.sh <version> <outdir>
set -eu
VER="$1"
OUT="$2"
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
BIN="$ROOT/target/x86_64-unknown-linux-musl/release/mwm"
[ -x "$BIN" ] || { echo "build first: cargo build --release --target x86_64-unknown-linux-musl -p mwm-cli"; exit 1; }
mkdir -p "$OUT"
cp "$BIN" "$OUT/mwm-linux-x86_64"
cp "$ROOT/packaging/linux/install.sh" "$OUT/install.sh"

# ---- .deb (Debian / Ubuntu / Proxmox VE) ----
P=$(mktemp -d)
chmod 0755 "$P"
mkdir -p "$P/DEBIAN" "$P/usr/bin" "$P/lib/systemd/system" "$P/usr/share/doc/mwm"
install -m 0755 "$BIN" "$P/usr/bin/mwm"
cat > "$P/lib/systemd/system/mwm-web.service" <<'EOF'
[Unit]
Description=MWM - Move Weight Manager web UI
After=network-online.target

[Service]
Environment=XDG_DATA_HOME=/var/lib
ExecStart=/usr/bin/mwm serve --bind 0.0.0.0:7777
Restart=on-failure

[Install]
WantedBy=multi-user.target
EOF
cp "$ROOT/README.md" "$P/usr/share/doc/mwm/README.md"
cat > "$P/DEBIAN/control" <<EOF
Package: mwm
Version: $VER
Section: admin
Priority: optional
Architecture: amd64
Maintainer: MoveWeight <hello@moveweight.com>
Recommends: smartmontools
Homepage: https://github.com/BlizzHacker/mwm
Description: MWM - Move Weight Manager: PC and server health, repair and recovery
 Junk cleaning, uninstall with leftovers, startup manager, updates, duplicate
 finder, disk analyzer, dual-pane file manager, keys vault, repair toolkit and
 a Proxmox / ZFS / SMART / Docker dashboard - on the command line or in a
 browser (mwm serve).
EOF
cat > "$P/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
command -v systemctl >/dev/null 2>&1 && systemctl daemon-reload || true
echo "MWM installed. Browser UI:  systemctl enable --now mwm-web  then  XDG_DATA_HOME=/var/lib mwm serve --show-token"
EOF
cat > "$P/DEBIAN/prerm" <<'EOF'
#!/bin/sh
set -e
command -v systemctl >/dev/null 2>&1 && systemctl disable --now mwm-web >/dev/null 2>&1 || true
EOF
chmod 0755 "$P/DEBIAN/postinst" "$P/DEBIAN/prerm"
dpkg-deb --root-owner-group --build "$P" "$OUT/mwm_${VER}_amd64.deb" >/dev/null
rm -rf "$P"

# ---- Unraid plugin ----
SHA=$(sha256sum "$BIN" | cut -d' ' -f1)
sed -e "s/@VERSION@/$VER/g" -e "s/@SHA256@/$SHA/g" "$ROOT/packaging/unraid/mwm.plg" > "$OUT/mwm.plg"

(cd "$OUT" && sha256sum mwm-linux-x86_64 "mwm_${VER}_amd64.deb" mwm.plg install.sh > SHA256SUMS)
ls -la "$OUT"
