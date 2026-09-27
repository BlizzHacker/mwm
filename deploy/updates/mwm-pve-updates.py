#!/usr/bin/env python3
"""Cluster-wide Proxmox host update inventory and guarded, serial upgrades."""
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

REPORT = pathlib.Path("/var/lib/mwm/pve-updates.json")
POLICY = pathlib.Path("/etc/mwm/pve-update-policy.json")
LOCK = pathlib.Path("/run/lock/mwm-pve-updates.lock")
CRITICAL = {"proxmox-ve", "pve-manager", "pve-cluster", "corosync"}


def run(args, timeout=120):
    return subprocess.run(args, text=True, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, timeout=timeout, check=False)


def pvesh(path):
    result = run(["pvesh", "get", path, "--output-format", "json"], 45)
    if result.returncode:
        raise RuntimeError(result.stderr.strip() or "pvesh failed")
    return json.loads(result.stdout)


def node_ips():
    members = json.loads(pathlib.Path("/etc/pve/.members").read_text())
    return {name: entry["ip"] for name, entry in members["nodelist"].items()}


def on_node(node, command, ips, timeout=180):
    if node not in ips:
        raise ValueError("node is not in this cluster")
    if node.lower() == socket.gethostname().lower():
        args = command
    else:
        args = ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=6",
                "root@" + ips[node], shlex.join(command)]
    return run(args, timeout)


def cluster_state():
    entries = pvesh("/cluster/status")
    cluster = next((x for x in entries if x.get("type") == "cluster"), {})
    nodes = sorted([x for x in entries if x.get("type") == "node"],
                   key=lambda x: x["name"].lower())
    return bool(cluster.get("quorate")), nodes


def backup_gate(nodes):
    """Block unattended host changes when an enabled PVE backup job is broken."""
    jobs = [x for x in pvesh("/cluster/backup")
            if str(x.get("enabled", 1)) != "0"]
    problems = []
    if not jobs:
        problems.append("no enabled Proxmox backup jobs")
    online = {x["name"] for x in nodes if x.get("online")}
    checked = set()
    for job in jobs:
        node = job.get("node")
        storage = job.get("storage")
        targets = [node] if node else sorted(online)
        if not storage:
            problems.append("backup job " + str(job.get("id", "?")) + " has no storage")
            continue
        for target in targets:
            if target not in online:
                problems.append("backup target " + str(target) + " is offline")
                continue
            key = (target, storage)
            if key in checked:
                continue
            checked.add(key)
            try:
                status = pvesh("/nodes/" + target + "/storage/" + storage + "/status")
                if not status.get("active"):
                    problems.append(target + ": backup storage " + storage + " inactive")
            except (RuntimeError, KeyError) as exc:
                problems.append(target + ": backup storage " + storage + " unavailable: " + str(exc)[:100])
    return {"ready": not problems, "problems": sorted(set(problems)), "jobs": len(jobs)}


def scan_one(entry, ips, refresh):
    node = entry["name"]
    row = {"node": node, "online": bool(entry.get("online")), "state": "",
           "version": "", "pending": 0, "packages": [], "reboot_required": False}
    if not row["online"]:
        row["state"] = "offline"
        return row
    script = ("if command -v apt-get >/dev/null 2>&1; then "
              + ("timeout 180 apt-get update -qq >/dev/null 2>&1 || echo MWM_REFRESH_FAILED; "
                 if refresh else "")
              + "pveversion | head -1; echo MWM_PACKAGES_BEGIN; "
              + "apt list --upgradable 2>/dev/null; echo MWM_PACKAGES_END; "
              + "test -e /var/run/reboot-required && echo MWM_REBOOT_REQUIRED || true; "
              + "else echo MWM_NO_APT; fi")
    try:
        result = on_node(node, ["sh", "-c", script], ips, 220)
        if result.returncode or "MWM_NO_APT" in result.stdout:
            row["state"] = "error"
            row["error"] = (result.stderr or result.stdout).strip()[-300:]
            return row
        lines = result.stdout.splitlines()
        row["version"] = next((x for x in lines if x.startswith("pve-manager/")), "")
        start = lines.index("MWM_PACKAGES_BEGIN")
        end = lines.index("MWM_PACKAGES_END")
        packages = [x.split("/", 1)[0] for x in lines[start + 1:end]
                    if "/" in x and "[upgradable from:" in x]
        row["pending"] = len(packages)
        row["packages"] = packages[:50]
        row["reboot_required"] = "MWM_REBOOT_REQUIRED" in lines
        row["state"] = "stale" if "MWM_REFRESH_FAILED" in lines else "ok"
    except (OSError, subprocess.TimeoutExpired, ValueError) as exc:
        row["state"] = "error"
        row["error"] = str(exc)[:300]
    return row


def write_report(report):
    REPORT.parent.mkdir(mode=0o750, parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix=".pve-updates-", dir=REPORT.parent)
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


def scan(refresh):
    quorate, nodes = cluster_state()
    ips = node_ips()
    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
        rows = list(pool.map(lambda entry: scan_one(entry, ips, refresh), nodes))
    try:
        backup = backup_gate(nodes)
    except RuntimeError as exc:
        backup = {"ready": False, "problems": [str(exc)[:200]], "jobs": 0}
    report = {"generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
              "quorate": quorate, "backup_gate": backup, "nodes": rows}
    write_report(report)
    print(json.dumps({"generated_at": report["generated_at"],
                      "nodes": len(rows), "pending": sum(x["pending"] for x in rows),
                      "backup_ready": backup["ready"], "quorate": quorate}))
    return report


def preflight(node):
    quorate, nodes = cluster_state()
    online = [x for x in nodes if x.get("online")]
    votes_needed = len(nodes) // 2 + 1
    if not quorate or len(online) != len(nodes) or len(online) - 1 < votes_needed:
        raise RuntimeError("cluster is not fully online or cannot retain quorum during node maintenance")
    if node not in {x["name"] for x in online}:
        raise RuntimeError("selected PVE node is not online")
    backup = backup_gate(nodes)
    if not backup["ready"]:
        raise RuntimeError("backup gate blocked host update: " + "; ".join(backup["problems"])[:400])
    return node_ips()


def apply(node):
    if not re.fullmatch(r"[A-Za-z0-9.-]{1,64}", node):
        raise ValueError("invalid node name")
    ips = preflight(node)
    result = on_node(node, ["apt-get", "update", "-qq"], ips, 300)
    if result.returncode:
        raise RuntimeError("APT metadata refresh failed: " + (result.stderr or result.stdout).strip()[-300:])
    simulation = on_node(node, ["apt-get", "-s", "dist-upgrade"], ips, 300)
    if simulation.returncode:
        raise RuntimeError("APT dry run failed: " + (simulation.stderr or simulation.stdout).strip()[-300:])
    removed = {line.split()[1] for line in simulation.stdout.splitlines()
               if line.startswith("Remv ") and len(line.split()) > 1}
    if removed & CRITICAL:
        raise RuntimeError("APT would remove critical PVE packages: " + ", ".join(sorted(removed & CRITICAL)))
    if not any(line.startswith("Inst ") for line in simulation.stdout.splitlines()):
        print(json.dumps({"node": node, "status": "current"}))
        return
    command = ["env", "DEBIAN_FRONTEND=noninteractive", "apt-get", "-y",
               "-o", "Dpkg::Options::=--force-confold", "dist-upgrade"]
    result = on_node(node, command, ips, 3600)
    if result.returncode:
        raise RuntimeError("PVE upgrade failed: " + (result.stderr or result.stdout).strip()[-500:])
    print(json.dumps({"node": node, "status": "updated", "reboot": "manual"}))


def auto(report):
    if not POLICY.exists():
        return
    policy = json.loads(POLICY.read_text())
    allowed = {str(x) for x in policy.get("auto_apply", [])}
    if not allowed:
        return
    if not report["quorate"] or not report["backup_gate"]["ready"]:
        print("Automatic PVE upgrades skipped: quorum or backup gate failed", file=sys.stderr)
        return
    for row in report["nodes"]:
        if row["node"] in allowed and row["state"] == "ok" and row["pending"] > 0:
            apply(row["node"])
            scan(False)
            return


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=["scan", "apply", "auto"])
    parser.add_argument("node", nargs="?")
    parser.add_argument("--refresh", action="store_true")
    args = parser.parse_args()
    if os.geteuid() != 0:
        parser.error("run as root on a Proxmox node")
    with LOCK.open("w") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            parser.error("PVE update job already running")
        if args.action == "apply":
            if not args.node:
                parser.error("apply requires NODE")
            apply(args.node)
            scan(False)
        else:
            report = scan(args.refresh)
            if args.action == "auto":
                auto(report)


if __name__ == "__main__":
    main()
