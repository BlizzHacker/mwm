#!/bin/sh
# Enable verified MWM binary updates on a Linux host, and optionally one
# cluster-wide LXC update scanner on a Proxmox node.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
service=""
cluster=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --service) service="$2"; shift ;;
    --cluster) cluster=1 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done
[ "$(id -u)" -eq 0 ] || { echo "run as root" >&2; exit 2; }
case "$service" in
  mwm-web|mwm-agent) ;;
  *) echo "use --service mwm-web or --service mwm-agent" >&2; exit 2 ;;
esac
command -v systemctl >/dev/null
systemctl is-active --quiet "$service" || { echo "$service is not active" >&2; exit 1; }
install -D -m 0755 "$root/mwm-self-update.sh" /usr/local/libexec/mwm-self-update
install -m 0644 "$root/mwm-self-update@.service" /etc/systemd/system/mwm-self-update@.service
install -m 0644 "$root/mwm-self-update@.timer" /etc/systemd/system/mwm-self-update@.timer
if [ "$cluster" -eq 1 ]; then
  command -v pvesh >/dev/null || { echo "--cluster requires a Proxmox node" >&2; exit 2; }
  install -D -m 0755 "$root/mwm-lxc-updates.py" /usr/local/libexec/mwm-lxc-updates
  install -m 0644 "$root/mwm-lxc-updates.service" /etc/systemd/system/mwm-lxc-updates.service
  install -m 0644 "$root/mwm-lxc-updates.timer" /etc/systemd/system/mwm-lxc-updates.timer
  install -d -m 0750 /etc/mwm
  if [ ! -e /etc/mwm/lxc-update-policy.json ]; then
    install -m 0640 "$root/lxc-update-policy.example.json" /etc/mwm/lxc-update-policy.json
  fi
fi
systemctl daemon-reload
systemctl enable --now "mwm-self-update@$service.timer"
if [ "$cluster" -eq 1 ]; then
  systemctl enable --now mwm-lxc-updates.timer
fi
echo "MWM timers enabled for $service; cluster inventory: $cluster"
