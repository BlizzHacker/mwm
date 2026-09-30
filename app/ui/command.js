/* MWM v5: one live view of Proxmox, installed apps, routes and Hestia domains. */
"use strict";

const CENTER = { tab: "fleet", query: "", pve: null, inventory: null, plugins: [], error: [] };
const centerArray = (v) => Array.isArray(v) ? v : [];
const centerSafeUrl = (v) => {
  try { const u = new URL(String(v)); return ["http:", "https:"].includes(u.protocol) ? u.href : ""; }
  catch { return ""; }
};
const centerKey = (node, vmid) => `${node || ""}:${vmid || ""}`;
const centerState = (v) => `<span class="pill ${v === "running" || v === "online" || v === "healthy" ? "low" : v === "stopped" ? "" : "medium"}">${esc(v || "unknown")}</span>`;

VIEWS.command = async function () {
  $("#top-actions").innerHTML = '<button class="btn" id="cc-refresh">Refresh inventory</button><button class="btn primary" id="cc-create">Create server</button>';
  $("#cc-refresh").onclick = () => VIEWS.command();
  $("#cc-create").onclick = () => centerCreateServer();
  $("#page").innerHTML = loading("Reading your Proxmox fleet, apps and domains...");
  const requests = await Promise.allSettled([
    invoke("pve_overview"), invoke("fleet_inventory"), invoke("plugins_list")
  ]);
  if (current !== "command") return;
  CENTER.pve = requests[0].status === "fulfilled" ? requests[0].value : null;
  CENTER.inventory = requests[1].status === "fulfilled" ? requests[1].value : null;
  CENTER.plugins = requests[2].status === "fulfilled" ? centerArray(requests[2].value) : [];
  CENTER.error = requests.map((r, i) => r.status === "rejected" ? `${["Proxmox", "Discovery", "App status"][i]}: ${String(r.reason)}` : "").filter(Boolean);
  drawCommand();
};

function centerData() {
  const inventory = CENTER.inventory?.available ? CENTER.inventory : null;
  const guests = centerArray(CENTER.pve?.guests);
  const known = new Map(guests.map((g) => [centerKey(g.node, g.vmid), g]));
  const containers = inventory ? centerArray(inventory.containers).map((c) => ({ ...known.get(centerKey(c.node, c.vmid)), ...c })) : guests;
  const nodes = inventory ? centerArray(inventory.nodes) : centerArray(CENTER.pve?.nodes);
  const apps = [];
  const seen = new Set();
  for (const c of containers) {
    for (const a of centerArray(c.apps)) {
      const app = typeof a === "string" ? { name: a } : a;
      const key = `${centerKey(c.node, c.vmid)}:${app.name || ""}`.toLowerCase();
      if (!app.name || seen.has(key)) continue;
      seen.add(key);
      const probe = CENTER.plugins.find((p) => String(p.vmid) === String(c.vmid) && p.node === c.node && (p.plugin || "").toLowerCase() === app.name.toLowerCase());
      apps.push({ ...app, node: c.node, vmid: c.vmid, container: c.name, role: c.role, state: c.status, url: app.url || probe?.url || "", reachable: probe?.info?.reachable, health: probe?.info?.health });
    }
  }
  for (const p of CENTER.plugins) {
    const key = `${centerKey(p.node, p.vmid)}:${p.plugin || ""}`.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    apps.push({ name: p.label || p.plugin, node: p.node, vmid: p.vmid, state: p.state, url: p.url, reachable: p.info?.reachable, health: p.info?.health });
  }
  return { inventory, nodes, containers, apps, services: centerArray(inventory?.services), routes: centerArray(inventory?.routes), domains: centerArray(inventory?.hestia?.domains), dns: centerArray(inventory?.hestia?.dns_zones) };
}

function drawCommand() {
  if (current !== "command") return;
  const d = centerData();
  const running = d.containers.filter((c) => c.status === "running").length;
  const attention = d.containers.filter((c) => c.status !== "running" && c.status !== "stopped").length;
  const hostnames = new Set(d.routes.flatMap((r) => centerArray(r.hostnames).length ? r.hostnames : r.hostname ? [r.hostname] : []));
  const tabs = [["fleet", `Servers & containers · ${d.containers.length}`], ["apps", `Apps & services · ${d.apps.length + d.services.length}`], ["domains", `Domains · ${hostnames.size}`], ["deploy", "Install & deploy"]];
  const fresh = d.inventory?.generated_at ? `Discovery ${new Date(d.inventory.generated_at).toLocaleString()}` : "Live Proxmox state";
  $("#page").innerHTML = `<div class="cc-hero card">
    <div><div class="label">MWM v5 · your infrastructure</div><h2>Command Center</h2><p>One place for the machines, LXCs, apps and domains behind your products.</p>
    <div class="row" style="gap:8px;flex-wrap:wrap"><span class="pill low">${esc(d.inventory?.cluster?.name || CENTER.pve?.cluster?.name || "Proxmox fleet")}</span><span class="muted">${esc(fresh)}</span></div></div>
    <div class="cc-hero-actions"><button class="btn primary" data-go="cluster">Open Proxmox</button><button class="btn" data-go="plugins">ARR & apps</button><button class="btn" data-go="fleet">Manage connections</button></div>
  </div>
  <div class="grid g4 cc-metrics">
    <div class="card"><div class="label">Nodes</div><div class="big">${d.nodes.length}</div><div class="muted">${d.nodes.filter((n) => n.status === "online").length} online</div></div>
    <div class="card"><div class="label">Guests running</div><div class="big">${running}<span class="cc-denom"> / ${d.containers.length}</span></div><div class="muted">${attention ? `${attention} needs review` : "Fleet status in sync"}</div></div>
    <div class="card"><div class="label">Apps & services</div><div class="big">${d.apps.length + d.services.length}</div><div class="muted">${d.apps.length} apps · ${d.services.length} endpoints</div></div>
    <div class="card"><div class="label">Public hostnames</div><div class="big">${hostnames.size}</div><div class="muted">${d.domains.length} Hestia web domains</div></div>
  </div>
  ${!d.inventory ? '<div class="note" style="margin:14px 0">The discovery feed is not connected yet. Live Proxmox guests are still shown; app and domain inventory will appear when a trusted discovery job publishes them.</div>' : ""}
  ${CENTER.error.length ? `<div class="note warn-n" style="margin:14px 0">${CENTER.error.map(esc).join(" · ")}</div>` : ""}
  <div class="tabs cc-tabs">${tabs.map(([id, label]) => `<button class="tab ${CENTER.tab === id ? "active" : ""}" data-cc-tab="${id}">${esc(label)}</button>`).join("")}</div>
  <div id="cc-body"></div>`;
  $$("[data-cc-tab]").forEach((b) => b.onclick = () => { CENTER.tab = b.dataset.ccTab; drawCommand(); });
  ({ fleet: drawCenterFleet, apps: drawCenterApps, domains: drawCenterDomains, deploy: drawCenterDeploy }[CENTER.tab])(d);
}

function centerSearch(rows, render, placeholder) {
  const box = $("#cc-body");
  box.innerHTML = `<input class="search cc-search" id="cc-search" placeholder="${esc(placeholder)}" value="${esc(CENTER.query)}"><div id="cc-results"></div>`;
  const input = $("#cc-search");
  const update = () => {
    CENTER.query = input.value;
    const q = CENTER.query.toLowerCase().trim();
    const filtered = q ? rows.filter((r) => JSON.stringify(r).toLowerCase().includes(q)) : rows;
    $("#cc-results").innerHTML = render(filtered);
    centerWireActions();
  };
  input.oninput = update;
  update();
}

function centerWireActions() {
  $$("[data-cc-guest]").forEach((el) => el.onclick = () => {
    if (typeof PVE !== "undefined") { PVE.q = el.dataset.ccGuest; PVE.tab = "guests"; }
    go("cluster");
  });
  $$("[data-cc-copy]").forEach((el) => el.onclick = () => copyText(el.dataset.ccCopy));
}

function drawCenterFleet(d) {
  centerSearch(d.containers, (rows) => {
    if (!rows.length) return '<div class="empty"><h3>No matching guests</h3>Try another name, node, role or ID.</div>';
    const nodes = d.nodes.length ? d.nodes : [...new Set(rows.map((c) => c.node))].map((name) => ({ name }));
    return nodes.map((n) => {
      const name = n.name || n.node;
      const onNode = rows.filter((c) => c.node === name);
      if (!onNode.length) return "";
      return `<div class="cc-node-heading"><h3>${esc(name)}</h3>${centerState(n.status || "online")}<span class="muted">${onNode.filter((c) => c.status === "running").length}/${onNode.length} running</span></div>
      <div class="cc-cards">${onNode.map((c) => `<button class="card cc-machine" data-cc-guest="${esc(c.vmid)}">
        <span class="cc-machine-top"><strong>${esc(c.name || `Guest ${c.vmid}`)}</strong>${centerState(c.status)}</span>
        <span class="muted mono">${esc(c.kind || c.type || "lxc").toUpperCase()} ${esc(c.vmid)} · ${esc(c.ipv4 || "no IP reported")}</span>
        <span class="cc-machine-meta"><span class="pill">${esc(c.role || "infrastructure")}</span>${centerArray(c.apps).slice(0, 3).map((a) => `<span class="pill">${esc(typeof a === "string" ? a : a.name)}</span>`).join("")}</span>
      </button>`).join("")}</div>`;
    }).join("");
  }, "Find a server, LXC, app, role or ID...");
}

function drawCenterApps(d) {
  const rows = [...d.apps, ...d.services.map((s) => ({ ...s, service: true }))];
  centerSearch(rows, (filtered) => filtered.length ? `<div class="cc-cards">${filtered.sort((a,b) => a.name.localeCompare(b.name)).map((a) => {
    const url = centerSafeUrl(a.url || centerArray(a.targets)[0]);
    const badge = a.service ? centerState(a.health === "up" ? "healthy" : a.health === "down" ? "unreachable" : "unknown")
      : a.reachable === true ? '<span class="pill low">API online</span>' : a.reachable === false ? '<span class="pill medium">API unreachable</span>' : centerState(a.state);
    return `<div class="card cc-app"><div class="spread"><strong>${esc(a.name)}</strong>${badge}</div>
      <div class="muted">${a.service ? `Traefik endpoint · ${esc(a.source || "")}` : `${esc(a.container || a.role || "Application")} · ${esc(a.node || "")} / ${esc(a.vmid || "")}`}</div>
      <div class="row" style="margin-top:12px">${a.vmid ? `<button class="btn small" data-cc-guest="${esc(a.vmid)}">Container</button>` : ""}${url ? `<a class="btn small" href="${esc(url)}" target="_blank" rel="noopener noreferrer">Open ↗</a>` : ""}</div></div>`;
  }).join("")}</div>` : '<div class="empty"><h3>No matching apps or services</h3>Discovery includes container apps and routed service endpoints.</div>', "Find Sonarr, Hestia, Jellyfin, a node or product...");
}

function drawCenterDomains(d) {
  const rows = d.routes.flatMap((r) => (centerArray(r.hostnames).length ? r.hostnames : r.hostname ? [r.hostname] : []).map((hostname) => ({ ...r, hostname })));
  centerSearch(rows, (filtered) => `<div class="grid g2 cc-domain-layout"><div class="card"><div class="spread"><h3>Traefik routes</h3><span class="pill">${filtered.length}</span></div>
    <div class="scroll cc-table"><table class="table"><thead><tr><th>Hostname</th><th>Product</th><th>Service</th><th>Status</th><th>Target</th></tr></thead><tbody>
    ${filtered.map((r) => `<tr><td class="cell-main">${esc(r.hostname)}<div class="cell-sub">${esc(r.migration_state || "")}</div></td><td>${esc(r.product || r.owner || "")}</td><td class="mono">${esc(r.service || "")}</td><td>${centerState(r.health === "up" ? "healthy" : r.health === "down" ? "unreachable" : "unknown")}</td><td class="muted">${esc(centerArray(r.target_containers).map((c) => `${c.name || c.vmid} on ${c.node}`).join(", ") || centerArray(r.targets)[0] || "")}</td></tr>`).join("")}</tbody></table></div></div>
    <div class="card"><div class="spread"><h3>HestiaCP</h3><span class="pill">${d.domains.length} web · ${d.dns.length} DNS</span></div>
      <p class="muted">Domains remain grouped by owner. Custom-domain migration targets appear here when planned.</p>
      <div class="scroll cc-table">${d.domains.map((x) => `<div class="cc-hestia-row"><div><strong>${esc(x.domain)}</strong><div class="cell-sub">${esc(String(x.hosting_role || "unclassified").replaceAll("_", " "))}</div></div><span class="pill">${esc(x.owner || "unknown")}</span></div>`).join("") || '<p class="muted">No Hestia inventory is connected.</p>'}</div></div></div>`, "Find hostname, product, service or target...");
  const products = centerArray(d.inventory?.domain_plan?.products);
  if (products.length) $("#cc-body").insertAdjacentHTML("afterbegin", `<div class="card cc-plan"><div class="spread"><div><div class="label">Product organization</div><h3>Canonical domains and dedicated LXCs</h3></div><span class="pill">${products.length} products</span></div>
    <div class="cc-product-grid">${products.map((p) => `<div class="cc-product"><strong>${esc(p.brand || p.id)}</strong><span class="muted">${esc(p.current_primary_domain || "No current domain")}</span>
      <span class="mono">${esc(p.lxc?.node || "")} · LXC ${esc(p.lxc?.vmid || "?")}</span>
      <span class="pill ${p.desired_primary_domain ? "low" : "medium"}">${esc(p.desired_primary_domain || "Future domain unset")}</span>
      <small class="muted">${esc(String(p.migration_state || "").replaceAll("_", " "))}</small></div>`).join("")}</div>
    <p class="muted" style="margin:12px 0 0">This is a plan and inventory. Domain changes need a chosen target and a checked cutover.</p></div>`);
}

function drawCenterDeploy(d) {
  $("#cc-body").innerHTML = `<div class="grid g2">
    <div class="card"><div class="label">Proxmox templates</div><h3>Spin up an LXC</h3><p class="muted">Choose a node, an existing trusted template, storage and network. MWM validates live resources before asking Proxmox to create it. A new server stays stopped until you start it.</p><button class="btn primary" id="cc-create-lxc">Create container…</button></div>
    <div class="card"><div class="label">Connect more machines</div><h3>Install MWM agent</h3><p class="muted">Install from the official GitHub release on another Proxmox or Linux host, then add its access link under Machines.</p><div class="mono cc-command" id="cc-install">curl -fsSL https://github.com/BlizzHacker/mwm/releases/latest/download/install.sh | sh -s -- --web</div><div class="row"><button class="btn" data-cc-copy="curl -fsSL https://github.com/BlizzHacker/mwm/releases/latest/download/install.sh | sh -s -- --web">Copy command</button><button class="btn" data-go="fleet">Add machine</button></div></div>
    <div class="card" style="grid-column:1/-1"><div class="label">Discovered applications</div><h3>Use the inventory before installing duplicates</h3><p class="muted">${d.apps.length} apps are already mapped to ${d.containers.length} guests. Check ARR and hosting status, backups and owner before placing another copy.</p><div class="row"><button class="btn" data-go="plugins">ARR & service plugins</button><button class="btn" data-go="cluster">Backups & updates</button></div></div>
  </div>`;
  $("#cc-create-lxc").onclick = centerCreateServer;
  centerWireActions();
}

async function centerCreateServer() {
  toast("Loading live Proxmox templates and storage...");
  const options = await guard(() => invoke("pve_provision_options"));
  if (!options) return;
  const nodes = centerArray(options.nodes).filter((n) => centerArray(n.templates).length && centerArray(n.rootfs_storage).length && centerArray(n.bridges).length);
  if (!nodes.length) return toast("No node has a template, container storage and a bridge available.", true);
  const first = nodes[0];
  modal(`<h2>Create a Proxmox LXC</h2><p class="muted">Uses an existing template. MWM creates an unprivileged container and leaves it stopped so you can review it first.</p>
    <div class="grid g2"><label class="muted">Node<select class="search" id="pv-new-node">${nodes.map((n) => `<option value="${esc(n.name)}">${esc(n.name)}</option>`).join("")}</select></label>
    <label class="muted">Guest ID<input class="search" id="pv-new-id" type="number" min="100" value="${esc(options.nextid || "")}"></label>
    <label class="muted">Hostname<input class="search" id="pv-new-name" placeholder="new-server"></label>
    <label class="muted">Template<select class="search" id="pv-new-template"></select></label>
    <label class="muted">Root storage<select class="search" id="pv-new-storage"></select></label>
    <label class="muted">Bridge<select class="search" id="pv-new-bridge"></select></label>
    <label class="muted">CPU cores<input class="search" id="pv-new-cores" type="number" min="1" max="32" value="2"></label>
    <label class="muted">Memory MiB<input class="search" id="pv-new-memory" type="number" min="256" max="131072" value="2048"></label>
    <label class="muted">Disk GiB<input class="search" id="pv-new-disk" type="number" min="4" max="2048" value="16"></label>
    <label class="muted">Root password<input class="search" id="pv-new-password" type="password" minlength="12" autocomplete="new-password" placeholder="12 or more characters"></label></div>
    <label class="row muted" style="margin-top:12px"><input type="checkbox" id="pv-new-nesting">Enable nesting (needed for Docker inside LXC)</label>
    <div id="pv-new-msg" class="muted" style="margin-top:10px"></div>
    <div class="row" style="justify-content:flex-end;margin-top:16px"><button class="btn" data-close>Cancel</button><button class="btn primary" id="pv-new-review">Review creation</button></div>`);
  const nodeSelect = $("#pv-new-node");
  const redrawChoices = () => {
    const node = nodes.find((n) => n.name === nodeSelect.value) || first;
    const choices = (id, values, label) => { $(id).innerHTML = values.map((v) => `<option value="${esc(typeof v === "string" ? v : v.volid)}">${esc(typeof v === "string" ? v : v.volid)}${typeof v === "object" && v.size ? ` · ${bytes(v.size)}` : ""}</option>`).join(""); };
    choices("#pv-new-template", node.templates);
    choices("#pv-new-storage", node.rootfs_storage);
    choices("#pv-new-bridge", node.bridges);
  };
  nodeSelect.onchange = redrawChoices; redrawChoices();
  $("#pv-new-review").onclick = async () => {
    const params = { node: nodeSelect.value, vmid: $("#pv-new-id").value, hostname: $("#pv-new-name").value.trim(), template: $("#pv-new-template").value, storage: $("#pv-new-storage").value, bridge: $("#pv-new-bridge").value,
      cores: Number($("#pv-new-cores").value), memory: Number($("#pv-new-memory").value), disk: Number($("#pv-new-disk").value), password: $("#pv-new-password").value, nesting: $("#pv-new-nesting").checked };
    if (!params.hostname || params.password.length < 12) { $("#pv-new-msg").textContent = "Enter a hostname and a root password of at least 12 characters."; return; }
    const confirmation = await choose(`Create ${esc(params.hostname)}?`, `<p class="muted">Container ${esc(params.vmid)} on ${esc(params.node)} from ${esc(params.template)}. ${params.cores} cores, ${params.memory} MiB RAM, ${params.disk} GiB on ${esc(params.storage)} via ${esc(params.bridge)}. It will remain stopped. Review backups and networking before using it.</p>`, [["go", "Create container", "primary"]]);
    if (!confirmation) return;
    await guard(async () => {
      const result = await invoke("pve_create_lxc", { params });
      closeModal();
      if (result.job) { toast(`Creating LXC ${params.vmid}. Follow progress in Activity.`); watchJob(result.job, () => { CENTER.pve = null; if (current === "command") VIEWS.command(); }); }
      else toast(result.message || "Container created.");
    });
  };
}
