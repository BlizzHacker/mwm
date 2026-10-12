#!/usr/bin/env bash
# MWM Rescue ISO builder - Linux (Debian live-build). Runs inside LXC190.
# Uses auto/config for lb settings. Static mwm binary + systemd rescue
# service are staged under files/. Produces out/MWM-Rescue-Linux.iso.
set -euo pipefail
cd /root/iso-linux

export LB_DISTRIBUTION=bookworm
lb config 2>&1 | tail -2

mkdir -p config/package-lists
cat > config/package-lists/mwm.list.chroot <<'EOF'
testdisk
gddrescue
smartmontools
chntpw
ntfs-3g
dosfstools
exfatprogs
btrfs-progs
xfsprogs
lvm2
mdadm
cryptsetup
parted
gdisk
ipmitool
ethtool
net-tools
openssh-client
curl
wget
ca-certificates
nano
htop
lm-sensors
dmidecode
pciutils
usbutils
memtester
stress-ng
ncdu
tree
p7zip-full
rsync
nmap
EOF

# Stage the MWM static binary + rescue service into the live filesystem.
# live-build reads config/includes.chroot/ (NOT a bare files/ tree).
mkdir -p config/includes.chroot/usr/local/bin
mkdir -p config/includes.chroot/etc/systemd/system/multi-user.target.wants
cp -f files/usr/local/bin/mwm config/includes.chroot/usr/local/bin/mwm
chmod 755 config/includes.chroot/usr/local/bin/mwm
if [ -f files/etc/systemd/system/mwm-rescue.service ]; then
  cp -f files/etc/systemd/system/mwm-rescue.service config/includes.chroot/etc/systemd/system/mwm-rescue.service
  ln -sf ../mwm-rescue.service config/includes.chroot/etc/systemd/system/multi-user.target.wants/mwm-rescue.service
fi

lb build 2>&1 | tail -6
mkdir -p out
# --image-name changes the produced filename; accept either name.
ISO_SRC="$(ls -t *.hybrid.iso live-image-amd64.hybrid.iso 2>/dev/null | head -1)"
[ -n "$ISO_SRC" ] || { echo "ERROR: no hybrid iso produced"; exit 1; }
cp -f "$ISO_SRC" out/MWM-Rescue-Linux.iso
sha256sum out/MWM-Rescue-Linux.iso > out/MWM-Rescue-Linux.iso.sha256
ls -la out/
echo LINUX-ISO-DONE
