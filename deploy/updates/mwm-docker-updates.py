#!/usr/bin/env python3
"""Agentless Docker Compose project inventory and opt-in updates inside LXCs."""
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

REPORT = pathlib.Path("/var/lib/mwm/docker-updates.json")
POLICY = pathlib.Path("/etc/mwm/docker-update-policy.json")
LOCK = pathlib.Path("/run/lock/mwm-docker-updates.lock")
MAX_AUTO_PER_RUN = 1


def run(args, timeout=120):
    return subprocess.run(args, text=True, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, timeout=timeout, check=False)


def pvesh(path):
    result = run(["pvesh", "get", path, "--output-format", "json"], 45)
    if result.returncode:
        raise RuntimeError(result.stderr.strip() or "pvesh failed")
    return json.loads(result.stdout)


def resources():
    result = run(["pvesh", "get", "/cluster/resources", "--type", "vm",
                  "--output-format", "json"], 45)
    if result.returncode:
        raise RuntimeError(result.stderr.strip() or "pvesh failed")
    return [x for x in json.loads(result.stdout) if x.get("type") == "lxc"]


def node_ips():
    members = json.loads(pathlib.Path("/etc/pve/.members").read_text())
    return {name: entry["ip"] for name, entry in members["nodelist"].items()}


def guest_command(node, vmid, script, ips):
    if not re.fullmatch(r"[0-9]{1,9}", vmid) or node not in ips:
        raise ValueError("invalid container target")
    if node.lower() == socket.gethostname().lower():
        return ["pct", "exec", vmid, "--", "sh", "-c", script]
    remote = "pct exec " + vmid + " -- sh -c " + shlex.quote(script)
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


def backup_ready(node, vmid):
    jobs = pvesh("/cluster/backup")
    for job in jobs:
        if str(job.get("enabled", 1)) == "0" or job.get("node") not in (None, node):
            continue
        ids = {x.strip() for x in str(job.get("vmid", "")).split(",")}
        if vmid not in ids and "all" not in ids and not job.get("all"):
            continue
        storage = job.get("storage")
        if not storage:
            continue
        try:
            status = pvesh("/nodes/" + node + "/storage/" + storage + "/status")
            if status.get("active"):
                return True
        except RuntimeError:
            pass
    return False


def projects_for(node, vmid, ips):
    script = ('if command -v docker >/dev/null 2>&1 && '
              'docker info >/dev/null 2>&1 && docker compose version >/dev/null 2>&1; then '
              'docker compose ls --format json; else echo "[]"; fi')
    result = run(guest_command(node, vmid, script, ips), 40)
    if result.returncode:
        raise RuntimeError((result.stderr or result.stdout).strip()[-300:])
    value = json.loads(result.stdout.strip() or "[]")
    if not isinstance(value, list):
        raise RuntimeError("Docker Compose returned unexpected inventory")
    return value


def scan_one(guest, ips):
    node, vmid = guest["node"], str(guest["vmid"])
    if guest.get("status") != "running":
        return []
    try:
        projects = projects_for(node, vmid, ips)
        backed = backup_ready(node, vmid) if projects else False
        return [{"node": node, "vmid": vmid, "container": guest.get("name", ""),
                 "project": str(x.get("Name", "")), "status": str(x.get("Status", "")),
                 "config_files": str(x.get("ConfigFiles", "")), "backup_ready": backed}
                for x in projects if x.get("Name")]
    except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired) as exc:
        return [{"node": node, "vmid": vmid, "container": guest.get("name", ""),
                 "project": "", "status": "error", "error": str(exc)[:300],
                 "backup_ready": False}]


def write_report(report):
    REPORT.parent.mkdir(mode=0o750, parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix=".docker-updates-", dir=REPORT.parent)
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


def scan():
    guests = resources()
    ips = node_ips()
    with concurrent.futures.ThreadPoolExecutor(max_workers=5) as pool:
        groups = list(pool.map(lambda guest: scan_one(guest, ips), guests))
    projects = sorted((x for group in groups for x in group),
                      key=lambda x: (int(x["vmid"]), x["project"]))
    report = {"generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
              "projects": projects}
    write_report(report)
    print(json.dumps({"generated_at": report["generated_at"],
                      "projects": sum(bool(x["project"]) for x in projects),
                      "errors": sum(x["status"] == "error" for x in projects)}))
    return report


def apply(node, vmid, project):
    if not re.fullmatch(r"[0-9]{1,9}", vmid) or not re.fullmatch(r"[A-Za-z0-9_.-]{1,100}", project):
        raise ValueError("invalid Compose target")
    ips = node_ips()
    guest = next((x for x in resources()
                  if x["node"] == node and str(x["vmid"]) == vmid), None)
    if not guest or guest.get("status") != "running":
        raise RuntimeError("selected LXC is not running")
    if not backup_ready(node, vmid):
        raise RuntimeError("no enabled backup job with active storage covers CT " + vmid)
    current = next((x for x in projects_for(node, vmid, ips)
                    if x.get("Name") == project), None)
    if not current:
        raise RuntimeError("Compose project is no longer present")
    files = [x.strip() for x in str(current.get("ConfigFiles", "")).split(",")]
    if not files or any(not x.startswith("/") or "\n" in x or "\r" in x for x in files):
        raise RuntimeError("Compose project has invalid config paths")
    base = pathlib.PurePosixPath(files[0]).parent.as_posix()
    args = " ".join("-f " + shlex.quote(x) for x in files)
    prefix = "cd " + shlex.quote(base) + " && docker compose -p " + shlex.quote(project) + " " + args
    images_result = run(guest_command(node, vmid, prefix + " config --images", ips), 90)
    if images_result.returncode:
        raise RuntimeError("cannot inspect Compose image list")
    images = sorted({x.strip() for x in images_result.stdout.splitlines() if x.strip()})
    if not images:
        raise RuntimeError("Compose project has no published images to pull")
    def image_ids():
        script = "docker image inspect --format '{{.Id}}' " + " ".join(shlex.quote(x) for x in images)
        result = run(guest_command(node, vmid, script, ips), 90)
        return result.stdout.strip()
    before = image_ids()
    pull = run(guest_command(node, vmid, prefix + " pull --ignore-buildable", ips), 1800)
    if pull.returncode:
        raise RuntimeError("Compose image pull failed; running services unchanged: " +
                           (pull.stderr or pull.stdout).strip()[-400:])
    after = image_ids()
    if before == after:
        print(json.dumps({"node": node, "vmid": vmid, "project": project, "status": "current"}))
        return
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%d%H%M%S")
    snap = "mwmimg" + stamp
    result = on_node(node, ["pct", "snapshot", vmid, snap,
                            "--description", "MWM before Docker Compose update"], ips, 900)
    if result.returncode:
        raise RuntimeError("snapshot failed; running services unchanged: " +
                           (result.stderr or result.stdout).strip()[-300:])
    result = run(guest_command(node, vmid, prefix + " up -d", ips), 1800)
    if result.returncode:
        raise RuntimeError("Compose update failed; snapshot " + snap +
                           " retained: " + (result.stderr or result.stdout).strip()[-500:])
    verify = run(guest_command(node, vmid, prefix + " ps --all --format json", ips), 90)
    if verify.returncode:
        raise RuntimeError("Compose status check failed; snapshot " + snap + " retained")
    lines = [json.loads(x) for x in verify.stdout.splitlines() if x.lstrip().startswith("{")]
    if not lines or any(x.get("State") != "running" or x.get("Health") == "unhealthy" for x in lines):
        raise RuntimeError("Compose service not running or unhealthy; snapshot " + snap + " retained")
    print(json.dumps({"node": node, "vmid": vmid, "project": project,
                      "snapshot": snap, "status": "updated", "services": len(lines)}))


def auto(report):
    if not POLICY.exists():
        return
    policy = json.loads(POLICY.read_text())
    allowed = {(str(x["vmid"]), str(x["project"]))
               for x in policy.get("auto_apply", [])}
    selected = [x for x in report["projects"]
                if (x["vmid"], x["project"]) in allowed
                and x["project"] and x["backup_ready"]
                and x["status"].startswith("running")]
    for row in selected[:MAX_AUTO_PER_RUN]:
        try:
            apply(row["node"], row["vmid"], row["project"])
        except Exception as exc:
            print(json.dumps({"vmid": row["vmid"], "project": row["project"],
                              "status": "failed", "error": str(exc)}), file=sys.stderr)
    if selected:
        scan()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=["scan", "apply", "auto"])
    parser.add_argument("node", nargs="?")
    parser.add_argument("vmid", nargs="?")
    parser.add_argument("project", nargs="?")
    args = parser.parse_args()
    if os.geteuid() != 0:
        parser.error("run as root on a Proxmox node")
    with LOCK.open("w") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            parser.error("Docker update job already running")
        if args.action == "apply":
            if not args.node or not args.vmid or not args.project:
                parser.error("apply requires NODE VMID PROJECT")
            apply(args.node, args.vmid, args.project)
            scan()
        else:
            report = scan()
            if args.action == "auto":
                auto(report)


if __name__ == "__main__":
    main()
