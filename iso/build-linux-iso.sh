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

lb build 2>&1 | tail -6
mkdir -p out
cp -f live-image-amd64.hybrid.iso out/MWM-Rescue-Linux.iso
sha256sum out/MWM-Rescue-Linux.iso > out/MWM-Rescue-Linux.iso.sha256
ls -la out/
echo LINUX-ISO-DONE
