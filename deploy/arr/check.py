"""Read-only MCP-ARR bridge smoke test on the host that runs MWM and MCP-ARR."""
import json
import os
from pathlib import Path
from urllib.request import Request, urlopen

token = Path("/root/.local/share/MWM/web-token").read_text().strip()
base_url = os.environ.get("MWM_URL", "http://127.0.0.1:7777").rstrip("/")


def call(command, args=None):
    request = Request(
        f"{base_url}/api/{command}",
        json.dumps(args or {}).encode(),
        headers={"X-MWM-Token": token, "Content-Type": "application/json"},
    )
    with urlopen(request, timeout=90) as response:
        return json.load(response)


print("configured", call("arr_mcp_configure", {"url": "http://127.0.0.1:3000/mcp"})["reachable"])
tools = call("arr_mcp_tools")
print("tools", len(tools), "arr_status", any(t["name"] == "arr_status" for t in tools))
result = call("arr_mcp_call", {"name": "arr_status", "args": {}})
print("arr_status_blocks", len(result.get("content", [])))
services = call("plugins_list")
for row in services:
    print("native", row["label"], row["state"], "reachable", row["info"].get("reachable"))
