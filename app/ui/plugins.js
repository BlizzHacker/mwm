/* Service plugins discovered across connected Proxmox hosts. */
"use strict";

const PLUG = { rows: [], error: "", mcp: null, mcpSource: "", mcpHost: "", hosts: 0 };

const arrMcpInvoke = (cmd, args = {}) => PLUG.mcpSource === TARGET
  ? invoke(cmd, args)
  : localInvoke("remote_call", { id: PLUG.mcpSource, cmd, args });

VIEWS.plugins = async function () {
  $("#top-actions").innerHTML = '<button class="btn" id="plug-refresh">Refresh</button>';
  $("#plug-refresh").onclick = () => VIEWS.plugins();
  $("#page").innerHTML = loading("Discovering services across connected Proxmox nodes...");
  const connections = arr(await localInvoke("conn_list").catch(() => []));
  const sources = [{ id: "", name: "This machine" }, ...connections];
  const [results, bridges] = await Promise.all([
    Promise.allSettled(sources.map((source) => source.id
      ? localInvoke("remote_call", { id: source.id, cmd: "plugins_list", args: {} })
      : localInvoke("plugins_list"))),
    Promise.allSettled(sources.map((source) => source.id
      ? localInvoke("remote_call", { id: source.id, cmd: "arr_mcp_status", args: {} })
      : localInvoke("arr_mcp_status"))),
  ]);
  const seen = new Set(), errors = [];
  PLUG.rows = [];
  PLUG.hosts = 0;
  results.forEach((result, i) => {
    if (result.status === "rejected") { if (sources[i].id === TARGET) errors.push(String(result.reason)); return; }
    if (arr(result.value).length) PLUG.hosts++;
    for (const row of arr(result.value)) {
      const key = `${row.node}|${row.vmid}|${row.plugin}`;
      if (seen.has(key)) continue;
      seen.add(key);
      PLUG.rows.push({ ...row, sourceId: sources[i].id });
    }
  });
  PLUG.rows.sort((a, b) => `${a.label}|${a.node}`.localeCompare(`${b.label}|${b.node}`));
  PLUG.error = errors.join(" · ");
  const selected = Math.max(0, sources.findIndex((source) => source.id === TARGET));
  const available = bridges.findIndex((result) => result.status === "fulfilled" && result.value?.reachable);
  const chosen = available >= 0 ? available : selected;
  PLUG.mcpSource = sources[chosen].id;
  PLUG.mcpHost = sources[chosen].name;
  PLUG.mcp = bridges[chosen].status === "fulfilled" ? bridges[chosen].value : { configured: false, reachable: false, error: String(bridges[chosen].reason) };
  if (current === "plugins") drawPlugins();
};

function drawPlugins() {
  const rows = PLUG.rows;
  $("#page").innerHTML = `<div class="note" style="margin-bottom:14px">${rows.length} services across ${PLUG.hosts} connected Proxmox node${PLUG.hosts === 1 ? "" : "s"}. API keys stay on the owning node. Right-click a service for actions.</div>
    ${rows.length ? `<div class="card" style="padding:0"><div class="scroll"><table class="table"><thead><tr><th>Service</th><th>Container</th><th>State</th><th>API</th><th>Health</th><th>Queue</th><th>Version</th></tr></thead><tbody>
    ${rows.map((r, i) => `<tr class="click" tabindex="0" data-plugin="${i}"><td class="cell-main">${esc(r.label)}</td><td class="mono">${esc(r.vmid)} · ${esc(r.node)}</td><td>${statePill(r.state)}</td><td>${r.info?.auth_required ? '<span class="pill medium">login required</span>' : r.info?.reachable ? '<span class="pill low">reachable</span>' : `<span class="pill ${r.state === "stopped" ? "" : "medium"}">${r.state === "stopped" ? "off" : "unreachable"}</span>`}</td><td>${r.info?.health ?? "—"}</td><td>${r.info?.queue ?? "—"}</td><td class="mono muted">${esc(r.info?.version || "")}</td></tr>`).join("")}
    </tbody></table></div></div>` : `<div class="empty"><h3>No supported services found</h3>Connect the owning Proxmox nodes in Machines, then refresh.${PLUG.error ? `<p>${esc(PLUG.error)}</p>` : ""}</div>`}
    ${rows.some((r) => r.info?.error) ? `<div class="card" style="margin-top:14px"><h3>API details</h3>${rows.filter((r) => r.info?.error).map((r) => `<div><b>${esc(r.label)}</b>: <span class="muted">${esc(r.info.error)}</span></div>`).join("")}</div>` : ""}`;
  $("#page").insertAdjacentHTML("beforeend", drawArrBridge());
  $("#arr-mcp-connect").onclick = connectArrBridge;
  $("#arr-mcp-tools").onclick = showArrTools;
  $$("[data-plugin]").forEach((tr) => {
    tr.onclick = () => pluginDetails(PLUG.rows[+tr.dataset.plugin]);
    tr.onkeydown = (e) => { if (e.key === "Enter") pluginDetails(PLUG.rows[+tr.dataset.plugin]); };
  });
}

function drawArrBridge() {
  const s = PLUG.mcp || {};
  return `<div class="card" style="margin-top:14px"><h3>MCP-ARR connection</h3>
    <p class="muted">Use MCP-ARR alongside MWM's built-in service view. Start its HTTP endpoint on the selected machine; MWM connects through localhost.</p>
    <div class="row" style="gap:8px;flex-wrap:wrap"><span class="pill ${s.reachable ? "low" : "medium"}">${s.reachable ? "connected" : s.configured ? "offline" : "not connected"}</span><span class="muted">via ${esc(PLUG.mcpHost)}</span><span class="mono muted">${esc(s.url || "127.0.0.1:3000/mcp")}</span></div>
    ${s.error ? `<p class="muted">${esc(s.error)}</p>` : ""}
    <div class="row" style="margin-top:12px;gap:8px"><button class="btn" id="arr-mcp-connect">${s.configured ? "Change endpoint" : "Connect MCP-ARR"}</button><button class="btn" id="arr-mcp-tools" ${s.reachable ? "" : "disabled"}>Browse tools</button></div></div>`;
}

async function connectArrBridge() {
  const url = await ask("Connect MCP-ARR", "Local Streamable HTTP endpoint", PLUG.mcp?.url || "http://127.0.0.1:3000/mcp", "Connect", "Start MCP-ARR with MCP_TRANSPORT=http on this machine. Keep it bound to 127.0.0.1.");
  if (!url) return;
  await guard(async () => { await arrMcpInvoke("arr_mcp_configure", { url }); toast("MCP-ARR connected."); await VIEWS.plugins(); });
}

async function showArrTools() {
  await guard(async () => {
    const tools = arr(await arrMcpInvoke("arr_mcp_tools"));
    modal(`<h2>MCP-ARR tools</h2><p class="muted">${tools.length} tools from your local MCP-ARR server. Calling a tool may change your media library; review its arguments first.</p><div class="scroll" style="max-height:58vh">${tools.map((t, i) => `<button class="btn" style="display:block;width:100%;text-align:left;margin:4px 0" data-arr-tool="${i}"><b>${esc(t.name)}</b><br><span class="muted">${esc(t.description || "")}</span></button>`).join("")}</div><div class="row" style="justify-content:flex-end;margin-top:12px"><button class="btn" data-close>Close</button></div>`);
    $$("[data-arr-tool]").forEach((b) => b.onclick = () => runArrTool(tools[+b.dataset.arrTool]));
  });
}

function runArrTool(tool) {
  modal(`<h2>${esc(tool.name)}</h2><p class="muted">${esc(tool.description || "")}</p><details><summary>Arguments expected by this tool</summary><pre class="mono wrap" style="max-height:26vh;overflow:auto">${esc(JSON.stringify(tool.inputSchema || {}, null, 2))}</pre></details><p class="muted">Arguments (JSON object)</p><textarea id="arr-mcp-args" class="search" style="width:100%;min-height:140px;font-family:monospace">{}</textarea><div class="row" style="justify-content:flex-end;margin-top:14px"><button class="btn" id="arr-mcp-cancel">Cancel</button><button class="btn primary" id="arr-mcp-run">Run tool</button></div>`);
  $("#arr-mcp-cancel").onclick = closeModal;
  $("#arr-mcp-run").onclick = async () => {
    let args;
    try {
      args = JSON.parse($("#arr-mcp-args").value);
      if (!args || Array.isArray(args) || typeof args !== "object") throw Error("Enter a JSON object");
    } catch (e) { toast(`Invalid arguments: ${e.message}`, true); return; }
    closeModal();
    await guard(async () => {
      const result = await arrMcpInvoke("arr_mcp_call", { name: tool.name, args });
      modal(`<h2>${esc(tool.name)} result</h2><pre class="mono wrap" style="max-height:60vh;overflow:auto">${esc(JSON.stringify(result, null, 2))}</pre><button class="btn" data-close>Close</button>`);
    });
  };
}

function pluginOpen(r) {
  if (!r.url) return;
  if (TAURI) invoke("open_default", { path: r.url });
  else window.open(r.url, "_blank", "noopener,noreferrer");
}

function pluginDetails(r) {
  if (!r) return;
  modal(`<h2>${esc(r.label)} <span class="muted" style="font-size:14px">LXC ${esc(r.vmid)} on ${esc(r.node)}</span></h2>
    <div class="grid g2"><div class="card"><div class="label">Container</div>${statePill(r.state)}</div><div class="card"><div class="label">Service API</div>${r.info?.reachable ? '<span class="pill low">reachable</span>' : '<span class="pill medium">unavailable</span>'}</div></div>
    <div class="mono wrap muted" style="margin:12px 0">${esc(r.url || "No address while stopped")}</div>
    ${r.info?.error ? `<div class="note warn-n">${esc(r.info.error)}</div>` : ""}
    <div class="row" style="margin-top:14px;flex-wrap:wrap"><button class="btn" id="plug-open" ${r.url ? "" : "disabled"}>Open service</button><button class="btn" id="plug-guest">Container details</button><button class="btn ${r.state === "running" ? "danger" : "primary"}" id="plug-power">${r.state === "running" ? "Shut down" : "Start"}</button><button class="btn" data-close>Close</button></div>`);
  $("#plug-open").onclick = () => pluginOpen(r);
  $("#plug-guest").onclick = () => pluginManage(r);
  $("#plug-power").onclick = () => pluginManage(r, r.state === "running" ? "shutdown" : "start");
}

async function pluginManage(r, action = "") {
  closeModal();
  if (TARGET !== r.sourceId) await switchTo(r.sourceId);
  if (action) await guestAction(r.node, "lxc", r.vmid, r.label, action);
  else await guestPanel(r.node, "lxc", r.vmid);
  if (action) setTimeout(() => { if (current === "plugins") VIEWS.plugins(); }, 1600);
}

function pluginMenu(r) {
  return [
    { label: "Details...", icon: "🔎", run: () => pluginDetails(r) },
    { label: "Open service", icon: "↗", disabled: !r.url, run: () => pluginOpen(r) },
    "-",
    { label: "Container details...", run: () => pluginManage(r) },
    { label: r.state === "running" ? "Shut down container..." : "Start container", run: () => pluginManage(r, r.state === "running" ? "shutdown" : "start") },
    "-",
    { label: "Copy address", disabled: !r.url, run: () => copyText(r.url) },
    { label: "Refresh", run: () => VIEWS.plugins() },
  ];
}
