#!/usr/bin/env python3
"""Install upstream MCP-ARR beside the local Proxmox *arr LXCs.

Run as root on the Proxmox node that owns the service containers. Keys are
read from guest config.xml files and written only to a root-owned env file;
this script never prints them. Docker administrators can inspect container
environment, so this host must be trusted like the guest hosts themselves.
"""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import xml.etree.ElementTree as ET

IMAGE = "ghcr.io/aplaceforallmystuff/mcp-arr:1.7.3@sha256:847156fc1c83a29f87da4d11a6bae35c0f16fc279e15a3bb5c9a58e398bcb9ef"
CONTAINER = "mwm-mcp-arr"
ENV_FILE = Path("/etc/mwm/mcp-arr.env")
SERVICES = {
    "sonarr": (8989, "/var/lib/sonarr/config.xml"),
    "radarr": (7878, "/var/lib/radarr/config.xml"),
    "lidarr": (8686, "/var/lib/lidarr/config.xml"),
    "prowlarr": (9696, "/var/lib/prowlarr/config.xml"),
}


def capture(*args):
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout.strip()


def main():
    if os.geteuid() != 0:
        raise SystemExit("Run as root on the Proxmox node")
    node = capture("hostname")
    guests = json.loads(capture("pvesh", "get", "/cluster/resources", "--type", "vm", "--output-format", "json"))
    values = {"MCP_TRANSPORT": "http", "HOST": "0.0.0.0", "PORT": "3000"}
    names = []
    for guest in guests:
        name = str(guest.get("name", "")).lower()
        if name not in SERVICES or guest.get("node") != node or guest.get("type") != "lxc" or guest.get("status") != "running":
            continue
        port, config = SERVICES[name]
        vmid = str(int(guest["vmid"]))
        try:
            xml = capture("pct", "exec", vmid, "--", "cat", config)
            key = ET.fromstring(xml).findtext("ApiKey", "").strip()
            ip = next(v for v in capture("pct", "exec", vmid, "--", "hostname", "-I").split()
                      if len(v.split(".")) == 4 and all(part.isdecimal() and 0 <= int(part) <= 255 for part in v.split(".")))
            if not key or any(c in key for c in "\r\n"):
                raise ValueError("invalid API key")
        except (subprocess.CalledProcessError, ET.ParseError, StopIteration, ValueError) as error:
            print(f"Skipping {name} LXC {vmid}: {type(error).__name__}")
            continue
        values[f"{name.upper()}_URL"] = f"http://{ip}:{port}"
        values[f"{name.upper()}_API_KEY"] = key
        names.append(name)
    if not names:
        raise SystemExit("No running supported *arr LXCs found on this node")

    ENV_FILE.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=".mcp-arr-", dir=ENV_FILE.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            for key, value in values.items():
                stream.write(f"{key}={value}\n")
        os.chmod(temporary, 0o600)
        os.replace(temporary, ENV_FILE)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)

    subprocess.run(["docker", "pull", IMAGE], check=True, stdout=subprocess.DEVNULL)
    subprocess.run(["docker", "rm", "-f", CONTAINER], check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    subprocess.run([
        "docker", "run", "-d", "--name", CONTAINER, "--restart", "unless-stopped",
        "-p", "127.0.0.1:3000:3000", "--env-file", str(ENV_FILE), IMAGE,
    ], check=True, stdout=subprocess.DEVNULL)
    print(f"MCP-ARR listening on 127.0.0.1:3000/mcp with: {', '.join(names)}")


if __name__ == "__main__":
    main()
