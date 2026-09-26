#!/usr/bin/env python3
"""Agentless cluster-wide LXC update inventory and controlled apt upgrades."""
import argparse
import concurrent.futures
import datetime as dt
import fcntl
import json
import os
import pathlib
import re
import shlex
import socket
import subprocess
import sys
import tempfile

REPORT = pathlib.Path("/var/lib/mwm/lxc-updates.json")
POLICY = pathlib.Path("/etc/mwm/lxc-update-policy.json")
APT_LIST = "apt list --upgradable 2>/dev/null"
MAX_AUTO_PER_RUN = 3


def run(args, timeout=120):
    return subprocess.run(args, text=True, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, timeout=timeout, check=False)


def resources():
    result = run(["pvesh", "get", "/cluster/resources", "--type", "vm",
                  "--output-format", "json"], 30)
    if result.returncode:
        raise RuntimeError(result.stderr.strip() or "pvesh failed")
    return [guest for guest in json.loads(result.stdout)
            if guest.get("type") == "lxc"]


def node_ips():
    members = json.loads(pathlib.Path("/etc/pve/.members").read_text())
    return {name: entry["ip"] for name, entry in members["nodelist"].items()}


def guest_command(node, vmid, script, ips):
    if not re.fullmatch(r"[0-9]{1,9}", str(vmid)):
        raise ValueError("invalid guest id")
    if node not in ips:
        raise ValueError("node is not in this cluster")
    if node.lower() == socket.gethostname().lower():
        return ["pct", "exec", str(vmid), "--", "sh", "-c", script]
    remote = "pct exec " + str(vmid) + " -- sh -c " + shlex.quote(script)
    return ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=6",
            "root@" + ips[node], remote]


def on_node(node, command, ips, timeout=300):
    if node not in ips:
        raise ValueError("node is not in this cluster")
    if node.lower() == socket.gethostname().lower():
        args = command
    else:
        args = ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=6",
                "root@" + ips[node], shlex.join(command)]
    return run(args, timeout)


def scan_one(guest, ips, refresh):
    vmid = str(guest["vmid"])
    row = {"node": guest["node"], "vmid": vmid,
           "name": guest.get("name") or "", "guest_status": guest.get("status"),
           "manager": "", "state": "", "pending": 0, "packages": []}
    if guest.get("status") != "running":
        row["state"] = "stopped"
        return row
    script = (
        "if command -v apt-get >/dev/null 2>&1; then "
        "echo MWM_MANAGER=apt; "
        + ("timeout 90 apt-get update -qq >/dev/null 2>&1 || echo MWM_REFRESH_FAILED; "
           if refresh else "")
        + APT_LIST + "; "
        "elif command -v apk >/dev/null 2>&1; then echo MWM_MANAGER=apk; "
        "else echo MWM_MANAGER=unsupported; fi"
    )
    try:
        result = run(guest_command(row["node"], vmid, script, ips), 125)
        if result.returncode:
            row["state"] = "error"
            row["error"] = (result.stderr or result.stdout).strip()[-300:]
            return row
        lines = result.stdout.splitlines()
        manager = next((line.split("=", 1)[1] for line in lines
                        if line.startswith("MWM_MANAGER=")), "")
        row["manager"] = manager
        if manager != "apt":
            row["state"] = "unsupported"
            return row
        packages = []
        for line in lines:
            if "[upgradable from:" in line and "/" in line:
                packages.append(line.split("/", 1)[0])
        row["pending"] = len(packages)
        row["packages"] = packages[:50]
        row["state"] = "stale" if "MWM_REFRESH_FAILED" in lines else "ok"
    except (OSError, subprocess.TimeoutExpired, ValueError) as exc:
        row["state"] = "error"
        row["error"] = str(exc)[:300]
    return row


def write_report(rows):
    REPORT.parent.mkdir(mode=0o750, parents=True, exist_ok=True)
    report = {"generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
              "containers": sorted(rows, key=lambda r: int(r["vmid"]))}
    fd, name = tempfile.mkstemp(prefix=".lxc-updates-", dir=REPORT.parent)
    try:
        with os.fdopen(fd, "w") as file:
            json.dump(report, file, separators=(",", ":"))
            file.write("\n")
            file.flush()
            os.fsync(file.fileno())
        os.chmod(name, 0o640)
        os.replace(name, REPORT)
    finally:
        if os.path.exists(name):
            os.unlink(name)
    return report


def scan(refresh):
    guests = resources()
    ips = node_ips()
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        rows = list(pool.map(lambda guest: scan_one(guest, ips, refresh), guests))
    report = write_report(rows)
    print(json.dumps({"generated_at": report["generated_at"], "total": len(rows),
                      "pending": sum(row["pending"] for row in rows),
                      "errors": sum(row["state"] == "error" for row in rows),
                      "stopped": sum(row["state"] == "stopped" for row in rows)}))
    return report


def snapshot_and_upgrade(node, vmid):
    ips = node_ips()
    guest = next((row for row in resources()
                  if str(row["vmid"]) == vmid and row["node"] == node), None)
    if not guest or guest.get("status") != "running":
        raise RuntimeError("container is not running on the selected node")
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%d%H%M")
    snap = "mwmupd" + stamp
    result = on_node(node, ["pct", "snapshot", vmid, snap,
                            "--description", "MWM before package update"], ips)
    if result.returncode:
        raise RuntimeError("snapshot failed; update skipped: " +
                           (result.stderr or result.stdout).strip()[-300:])
    script = ("command -v apt-get >/dev/null 2>&1 || exit 42; "
              "export DEBIAN_FRONTEND=noninteractive; "
              "apt-get update -qq && "
              "apt-get upgrade -y -o Dpkg::Options::=--force-confold")
    result = run(guest_command(node, vmid, script, ips), 1800)
    if result.returncode:
        raise RuntimeError("apt upgrade failed; snapshot " + snap +
                           " retained: " + (result.stderr or result.stdout).strip()[-300:])
    print(json.dumps({"node": node, "vmid": vmid, "snapshot": snap,
                      "status": "updated"}))


def auto_apply(report):
    if not POLICY.exists():
        return
    policy = json.loads(POLICY.read_text())
    allowed = {str(item) for item in policy.get("auto_apply", [])}
    if not allowed:
        return
    candidates = [row for row in report["containers"]
                  if row["vmid"] in allowed and row["state"] == "ok"
                  and row["pending"] > 0]
    for row in candidates[:MAX_AUTO_PER_RUN]:
        try:
            snapshot_and_upgrade(row["node"], row["vmid"])
        except Exception as exc:
            print(json.dumps({"node": row["node"], "vmid": row["vmid"],
                              "status": "failed", "error": str(exc)}), file=sys.stderr)
    if candidates:
        scan(False)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=["scan", "apply", "auto"])
    parser.add_argument("node", nargs="?")
    parser.add_argument("vmid", nargs="?")
    parser.add_argument("--refresh", action="store_true")
    args = parser.parse_args()
    if os.geteuid() != 0:
        parser.error("run as root on a Proxmox node")
    with open("/run/lock/mwm-lxc-updates.lock", "w") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            parser.error("an LXC update job is already running")
        if args.action == "apply":
            if not args.node or not args.vmid:
                parser.error("apply requires NODE VMID")
            snapshot_and_upgrade(args.node, args.vmid)
        else:
            report = scan(args.refresh)
            if args.action == "auto":
                auto_apply(report)


if __name__ == "__main__":
    main()
