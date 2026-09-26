/* MWM - Proxmox Cluster: real multi-node management through the cluster API.
   Guests (power, snapshots, backups, migration, Docker), nodes, storage,
   backup coverage, tasks and HA. Long operations are followed as MWM jobs. */
"use strict";

const PVE = { ov: null, tab: "guests", q: "", node: "", state: "", sort: { k: "vmid", d: 1 }, updates: null, updatePolicy: null, updateOwner: null };
const ago = (t) => { if (!t) return ""; const s = Date.now() / 1000 - t; return s < 3600 ? `${Math.round(s / 60)} min ago` : s < 86400 ? `${Math.round(s / 3600)} h ago` : `${Math.round(s / 86400)} d ago`; };
const when = (t) => (t ? new Date(t * 1000).toLocaleString() : "");
const pctv = (a, b) => (b ? Math.round((a / b) * 100) : 0);

PAGES.splice(PAGES.findIndex((p) => p.id === "server"), 0, { id: "cluster", label: "Proxmox Cluster", icon: "server" });

VIEWS.cluster = async function () {
  $("#top-actions").innerHTML = `<button class="btn" id="pv-re">Refresh</button>`;
  $("#pv-re").onclick = () => { PVE.ov = null; PVE.updates = null; VIEWS.cluster(); };
  if (!PVE.ov) {
    $("#page").innerHTML = loading("Reading the whole cluster: nodes, guests, storage, backups, tasks...");
    try { PVE.ov = await invoke("pve_overview"); }
    catch (e) {
      if (current !== "cluster") return;
      $("#page").innerHTML = `<div class="empty">${icon("server")}<h3>Not connected to a Proxmox cluster</h3>${esc(String(e))}<br><br>Install MWM on any node (<span class="mono">install.sh --web</span>) and pick it in the machine switcher - one node shows the whole cluster.</div>`;
      return;
    }
    if (current !== "cluster") return;
  }
  drawCluster();
};

function backupAge(g) {
  const b = g.backup || {};
  if (!b.ok) return { cls: "high", text: b.failed ? `failed ${ago(b.failed)}` : "none found" };
  const age = Date.now() / 1000 - b.ok;
  return { cls: age > 7 * 86400 ? "medium" : "low", text: ago(b.ok) };
}

function drawCluster() {
  const o = PVE.ov;
  const nodes = arr(o.nodes), guests = arr(o.guests), storage = arr(o.storage), tasks = arr(o.tasks);
  const online = nodes.filter((n) => n.status === "online").length;
  const running = guests.filter((g) => g.status === "running").length;
  const badTasks = tasks.filter((t) => t.status && t.status !== "OK");
  const badStore = storage.filter((s) => s.status !== "available");
  const noBackup = guests.filter((g) => !g.template && backupAge(g).cls !== "low");
  const tabs = [["guests", `Guests (${guests.length})`], ["nodes", `Nodes (${nodes.length})`], ["storage", `Storage${badStore.length ? ` ⚠${badStore.length}` : ""}`], ["backups", `Backups${noBackup.length ? ` ⚠${noBackup.length}` : ""}`], ["lxc_updates", "Updates"], ["tasks", `Tasks${badTasks.length ? ` ⚠${badTasks.length}` : ""}`], ["ha", "HA"]];
  $("#page").innerHTML = `
    <div class="grid g4">
      <div class="card"><div class="label">Cluster</div><div class="v" style="font-size:20px;font-weight:700;margin:6px 0">${esc(o.cluster?.name || "standalone")}</div><div>${o.quorate ? '<span class="pill low">quorate</span>' : '<span class="pill high">NO QUORUM</span>'}</div></div>
      <div class="card"><div class="label">Nodes online</div><div class="v ${online < nodes.length ? "danger-t" : "accent"}" style="font-size:22px;font-weight:700;margin:6px 0">${online} / ${nodes.length}</div><div class="muted">${nodes.map((n) => esc(n.node)).join(" · ")}</div></div>
      <div class="card"><div class="label">Guests running</div><div class="v" style="font-size:22px;font-weight:700;margin:6px 0">${running} / ${guests.length}</div><div class="muted">${guests.filter((g) => g.type === "qemu").length} VMs · ${guests.filter((g) => g.type === "lxc").length} containers</div></div>
      <div class="card"><div class="label">Needs attention</div><div class="v ${noBackup.length + badStore.length + badTasks.length ? "warn" : "accent"}" style="font-size:22px;font-weight:700;margin:6px 0">${noBackup.length + badStore.length + badTasks.length}</div><div class="muted">${noBackup.length} without recent backup · ${badStore.length} storage · ${badTasks.length} failed tasks</div></div>
    </div>
    <div class="tabs" style="margin-top:16px">${tabs.map(([k, l]) => `<button class="tab ${PVE.tab === k ? "active" : ""}" data-pvt="${k}">${l}</button>`).join("")}</div>
    <div id="pv-body"></div>`;
  $$("[data-pvt]").forEach((b) => (b.onclick = () => { PVE.tab = b.dataset.pvt; drawCluster(); }));
  ({ guests: drawGuests, nodes: drawNodesTab, storage: drawStorageTab, backups: drawBackupsTab, lxc_updates: drawLxcUpdatesTab, tasks: drawTasksTab, ha: drawHaTab }[PVE.tab])();
}

// ------------------------------------------------------------ guests ----
function drawGuests() {
  const o = PVE.ov;
  const q = PVE.q.toLowerCase();
  let g = arr(o.guests).filter((x) => (!q || `${x.vmid} ${x.name} ${x.tags || ""}`.toLowerCase().includes(q)) && (!PVE.node || x.node === PVE.node) && (!PVE.state || x.status === PVE.state));
  const { k, d } = PVE.sort;
  const val = (x) => (k === "backup" ? (x.backup?.ok || 0) : k === "mem" ? x.mem || 0 : k === "cpu" ? x.cpu || 0 : x[k] ?? "");
  g = g.sort((a, b) => (val(a) < val(b) ? -d : val(a) > val(b) ? d : 0));
  const th = (key, label, cls = "") => `<th class="sort ${cls}" data-pvs="${key}">${label}${k === key ? (d > 0 ? " ▴" : " ▾") : ""}</th>`;
  $("#pv-body").innerHTML = `
    <div class="row" style="margin-bottom:10px;flex-wrap:wrap">
      <input class="search" id="pv-q" placeholder="Search id, name, tag..." value="${esc(PVE.q)}">
      <select class="search" id="pv-node"><option value="">All nodes</option>${arr(o.nodes).map((n) => `<option ${PVE.node === n.node ? "selected" : ""}>${esc(n.node)}</option>`).join("")}</select>
      <select class="search" id="pv-state"><option value="">Any state</option>${["running", "stopped", "paused"].map((s) => `<option ${PVE.state === s ? "selected" : ""}>${s}</option>`).join("")}</select>
      <span class="muted">${g.length} shown · right-click a guest for actions</span></div>
    <div class="card" style="padding:0"><div class="scroll"><table class="table"><thead><tr>${th("vmid", "ID")}${th("name", "Name")}${th("node", "Node")}<th>Type</th>${th("status", "Status")}${th("cpu", "CPU", "num")}${th("mem", "Memory", "num")}${th("uptime", "Uptime", "num")}${th("backup", "Last backup")}</tr></thead><tbody>
    ${g.map((x) => { const b = backupAge(x); return `<tr class="click" data-guest="${esc(x.node)}|${esc(x.type)}|${x.vmid}">
      <td class="mono">${x.vmid}</td><td class="cell-main">${esc(x.name || "")}${x.template ? ' <span class="pill">template</span>' : ""}${x.tags ? `<div class="cell-sub">${esc(String(x.tags).replace(/;/g, " · "))}</div>` : ""}</td>
      <td class="muted">${esc(x.node)}</td><td class="muted">${x.type === "lxc" ? "container" : "VM"}</td><td>${statePill(x.status || "")}${x.hastate ? ` <span class="pill">HA ${esc(x.hastate)}</span>` : ""}</td>
      <td class="num">${x.status === "running" ? ((x.cpu || 0) * 100).toFixed(0) + "%" : ""}</td><td class="num">${x.maxmem ? `${bytes(x.mem || 0)} / ${bytes(x.maxmem)}` : ""}</td><td class="num muted">${upt(x.uptime)}</td>
      <td>${x.template ? "" : `<span class="pill ${b.cls}">${esc(b.text)}</span>`}</td></tr>`; }).join("")}
    </tbody></table></div></div>`;
  $("#pv-q").oninput = (e) => { PVE.q = e.target.value; const pos = e.target.selectionStart; drawGuests(); const i = $("#pv-q"); i.focus(); i.setSelectionRange(pos, pos); };
  $("#pv-node").onchange = (e) => { PVE.node = e.target.value; drawGuests(); };
  $("#pv-state").onchange = (e) => { PVE.state = e.target.value; drawGuests(); };
  $$("[data-pvs]").forEach((h) => (h.onclick = () => { const key = h.dataset.pvs; PVE.sort = { k: key, d: PVE.sort.k === key ? -PVE.sort.d : 1 }; drawGuests(); }));
  $$("[data-guest]").forEach((r) => (r.onclick = () => { const [n, t, v] = r.dataset.guest.split("|"); guestPanel(n, t, v); }));
}

/** Run a guest operation; long ones become tracked jobs. */
async function pveOp(node, kind, vmid, op, params = {}, label = op) {
  const r = await guard(() => invoke("pve_op", { node, kind, vmid: String(vmid), op, params }));
  if (!r) return false;
  if (r.job) {
    toast(`${label} started - follow it in the sidebar.`);
    watchJob(r.job, () => { PVE.ov = null; if (current === "cluster") VIEWS.cluster(); });
  } else {
    toast(r.message || `${label}: done`);
    PVE.ov = null;
    setTimeout(() => current === "cluster" && VIEWS.cluster(), 1200);
  }
  return true;
}

async function confirmOp(title, body, btn, danger = false) {
  return choose(title, body, [["go", btn, danger ? "danger" : "primary"]]);
}

async function guestAction(node, kind, vmid, name, op) {
  const labels = { start: "Start", shutdown: "Shut down", reboot: "Reboot", stop: "Force stop", suspend: "Pause", resume: "Resume" };
  const danger = op === "stop";
  if (op !== "start" && op !== "resume" && !(await confirmOp(`${labels[op]} ${esc(name)} (${vmid})?`, danger ? '<p class="muted">Force stop is like pulling the power cord - unsaved data inside the guest is lost. Prefer Shut down.</p>' : `<p class="muted">On node ${esc(node)}.</p>`, labels[op], danger))) return;
  await pveOp(node, kind, vmid, op, {}, `${labels[op]} ${name}`);
}

async function snapshotDialog(node, kind, vmid, name) {
  const def = `mwm-${new Date().toISOString().slice(0, 16).replace(/[-:T]/g, "")}`;
  modal(`<h2>Snapshot ${esc(name)} (${vmid})</h2><p class="muted">A snapshot freezes the guest's disks (and optionally RAM) so you can roll back if an update goes wrong.</p>
    <label class="muted">Name<input class="search mono" id="sn-n" style="width:100%;margin:4px 0 10px" value="${def}"></label>
    <label class="muted">Description<input class="search" id="sn-d" style="width:100%;margin:4px 0 10px" placeholder="before upgrade"></label>
    ${kind === "qemu" ? '<label class="row muted" style="gap:6px"><input type="checkbox" class="cb" id="sn-ram">Include RAM (resume exactly where it was)</label>' : ""}
    <div class="row" style="justify-content:flex-end;margin-top:14px"><button class="btn" data-close>Cancel</button><button class="btn primary" id="sn-go">Take snapshot</button></div>`);
  $("#sn-go").onclick = async () => {
    const params = { name: $("#sn-n").value.trim(), description: $("#sn-d").value.trim(), vmstate: !!$("#sn-ram")?.checked };
    closeModal();
    await pveOp(node, kind, vmid, "snapshot", params, `Snapshot ${name}`);
  };
}

async function backupDialog(node, kind, vmid, name, storages) {
  if (!storages) {
    const d = await guard(() => invoke("pve_guest", { node, kind, vmid: String(vmid) }));
    if (!d) return;
    storages = arr(d.backup_storages);
  }
  if (!storages.length) {
    modal(`<h2>No backup storage on ${esc(node)}</h2><p class="muted">No active storage on this node accepts backups. In Proxmox, enable a storage with content type <b>VZDump backup file</b> (or fix the inactive one) and try again.</p><div class="row" style="justify-content:flex-end"><button class="btn primary" data-close>OK</button></div>`);
    return;
  }
  modal(`<h2>Back up ${esc(name)} (${vmid}) now</h2>
    <label class="muted">Storage<select class="search" id="bk-s" style="width:100%;margin:4px 0 10px">${storages.map((s) => `<option>${esc(s)}</option>`).join("")}</select></label>
    <label class="muted">Mode<select class="search" id="bk-m" style="width:100%;margin-top:4px"><option value="snapshot">Snapshot (no downtime)</option><option value="suspend">Suspend (brief pause)</option><option value="stop">Stop (most consistent, downtime)</option></select></label>
    <div class="row" style="justify-content:flex-end;margin-top:14px"><button class="btn" data-close>Cancel</button><button class="btn primary" id="bk-go">Start backup</button></div>`);
  $("#bk-go").onclick = async () => {
    const params = { storage: $("#bk-s").value, mode: $("#bk-m").value };
    closeModal();
    await pveOp(node, kind, vmid, "backup", params, `Backup ${name}`);
  };
}

async function migrateDialog(node, kind, vmid, name, running) {
  const targets = arr(PVE.ov?.nodes).filter((n) => n.status === "online" && n.node !== node);
  if (!targets.length) return toast("No other online node to migrate to.", true);
  modal(`<h2>Migrate ${esc(name)} (${vmid})</h2>
    <p class="muted">From <b>${esc(node)}</b> to another node. ${running ? (kind === "qemu" ? "The VM moves live (no downtime) if its disks are on shared storage." : "The container is stopped, moved and restarted on the target.") : "The guest is stopped, so it moves offline."}</p>
    <label class="muted">Target node<select class="search" id="mg-t" style="width:100%;margin-top:4px">${targets.map((n) => `<option>${esc(n.node)}</option>`).join("")}</select></label>
    <div class="row" style="justify-content:flex-end;margin-top:14px"><button class="btn" data-close>Cancel</button><button class="btn primary" id="mg-go">Migrate</button></div>`);
  $("#mg-go").onclick = async () => {
    const target = $("#mg-t").value;
    closeModal();
    await pveOp(node, kind, vmid, "migrate", { target, running }, `Migrate ${name} → ${target}`);
  };
}

async function guestPanel(node, kind, vmid) {
  modal(loading(`Loading guest ${vmid}...`));
  $("#modal-card").classList.add("wide");
  const d = await guard(() => invoke("pve_guest", { node, kind, vmid: String(vmid) }));
  if (!d) return closeModal();
  const st = d.status || {}, cfg = d.config || {};
  const name = st.name || cfg.hostname || cfg.name || vmid;
  const running = st.status === "running";
  const snaps = arr(d.snapshots).filter((s) => s.name !== "current");
  const disks = Object.entries(cfg).filter(([k]) => /^(rootfs|mp\d+|scsi\d+|virtio\d+|sata\d+|ide\d+|efidisk\d+)$/.test(k));
  const nets = Object.entries(cfg).filter(([k]) => /^net\d+$/.test(k));
  modal(`<div class="spread"><h2 style="margin:0">${esc(name)} <span class="muted" style="font-size:15px">${kind === "lxc" ? "container" : "VM"} ${vmid} on ${esc(node)}</span></h2>${statePill(st.status || "")}</div>
    <div class="row" style="flex-wrap:wrap;margin:12px 0">
      ${running ? `<button class="btn small" data-g="shutdown">Shut down</button><button class="btn small" data-g="reboot">Reboot</button><button class="btn small danger" data-g="stop">Force stop</button>${kind === "qemu" ? '<button class="btn small" data-g="suspend">Pause</button>' : ""}` : st.status === "paused" ? '<button class="btn small primary" data-g="resume">Resume</button>' : '<button class="btn small primary" data-g="start">Start</button>'}
      <button class="btn small" id="gp-snap">Snapshot...</button><button class="btn small" id="gp-bk">Back up now...</button><button class="btn small" id="gp-mg">Migrate...</button>
      ${kind === "lxc" && running ? '<button class="btn small" id="gp-dk">Docker...</button><button class="btn small" id="gp-files">Files...</button>' : ""}</div>
    <div class="grid g2">
      <div class="card"><div class="label">Live</div>${kv([
        ["CPU", running ? `${((st.cpu || 0) * 100).toFixed(1)}% of ${st.cpus || cfg.cores || "?"} cores` : "-"],
        ["Memory", st.maxmem ? `${bytes(st.mem || 0)} / ${bytes(st.maxmem)}` : ""],
        ["Disk", st.maxdisk ? `${bytes(st.disk || 0)} / ${bytes(st.maxdisk)}` : ""],
        ["Network", running ? `↓ ${bytes(st.netin || 0)} · ↑ ${bytes(st.netout || 0)}` : ""],
        ["Uptime", running ? upt(st.uptime) : ""],
        ["HA", st.ha?.managed ? esc(st.ha.state || "managed") : ""],
      ])}</div>
      <div class="card"><div class="label">Configuration</div>${kv([
        ["OS", esc(cfg.ostype || "")],
        ["Cores / memory", `${esc(cfg.cores || cfg.sockets || "?")} cores · ${cfg.memory ? bytes(cfg.memory * 1048576) : "?"}`],
        ["Start on boot", cfg.onboot ? "yes" : "no"],
        ...disks.map(([k, v]) => [k, `<span class="mono" style="font-size:12px">${esc(String(v).slice(0, 90))}</span>`]),
        ...nets.map(([k, v]) => [k, `<span class="mono" style="font-size:12px">${esc(String(v).slice(0, 90))}</span>`]),
        ["Tags", esc(String(cfg.tags || "").replace(/;/g, " · "))],
      ])}</div>
    </div>
    <div class="label" style="margin:16px 0 8px">Snapshots (${snaps.length})</div>
    ${snaps.length ? `<div class="card" style="padding:0"><table class="table"><tbody>${snaps.map((s) => `<tr><td class="cell-main mono">${esc(s.name)}${s.vmstate ? ' <span class="pill">+RAM</span>' : ""}<div class="cell-sub">${esc(s.description || "")}</div></td><td class="muted">${when(s.snaptime)}</td>
      <td style="text-align:right;white-space:nowrap"><button class="btn small" data-rb="${esc(s.name)}">Roll back</button> <button class="btn small danger" data-ds="${esc(s.name)}">Delete</button></td></tr>`).join("")}</tbody></table></div>` : '<div class="muted">No snapshots.</div>'}
    <div class="label" style="margin:16px 0 8px">Backups on disk (${arr(d.backups).length})</div>
    ${arr(d.backups).length ? `<div class="card" style="padding:0"><div class="scroll" style="max-height:26vh"><table class="table"><tbody>${arr(d.backups).map((b) => `<tr><td class="mono" style="font-size:12px">${esc(b.volid)}</td><td class="muted">${when(b.ctime)}</td><td class="num">${bytes(b.size || 0)}</td></tr>`).join("")}</tbody></table></div></div>`
      : `<div class="note warn-n">No backups of this guest on any active backup storage${arr(d.backup_storages).length ? "" : " - and this node has no active backup storage at all"}.</div>`}
    <div class="row" style="justify-content:flex-end;margin-top:14px"><button class="btn" data-close>Close</button></div>`);
  $("#modal-card").classList.add("wide");
  $$("[data-g]").forEach((b) => (b.onclick = () => { closeModal(); guestAction(node, kind, vmid, name, b.dataset.g); }));
  $("#gp-snap").onclick = () => snapshotDialog(node, kind, vmid, name);
  $("#gp-bk").onclick = () => backupDialog(node, kind, vmid, name, arr(d.backup_storages));
  $("#gp-mg").onclick = () => migrateDialog(node, kind, vmid, name, running);
  $("#gp-dk") && ($("#gp-dk").onclick = () => dockerPanel(node, vmid, name));
  $("#gp-files") && ($("#gp-files").onclick = () => lxcFilePanel(node, vmid, "/"));
  $$("[data-rb]").forEach((b) => (b.onclick = async () => {
    if (!(await confirmOp(`Roll back ${esc(name)} to ${esc(b.dataset.rb)}?`, "<p class=\"muted\">Everything that changed since that snapshot is lost. The guest may be stopped during rollback.</p>", "Roll back", true))) return;
    await pveOp(node, kind, vmid, "rollback", { name: b.dataset.rb }, `Rollback ${name}`);
  }));
  $$("[data-ds]").forEach((b) => (b.onclick = async () => {
    if (!(await confirmOp(`Delete snapshot ${esc(b.dataset.ds)}?`, "<p class=\"muted\">The guest keeps running; you just can't roll back to this point any more.</p>", "Delete", true))) return;
    await pveOp(node, kind, vmid, "delsnapshot", { name: b.dataset.ds }, `Delete snapshot ${b.dataset.ds}`);
  }));
}

// ------------------------------------------------------------- nodes ----
function drawNodesTab() {
  $("#pv-body").innerHTML = `<div class="grid g3">${arr(PVE.ov.nodes).map((n) => `<div class="card mcard">
    <div class="spread"><b>${esc(n.node)}</b>${statePill(n.status === "online" ? "online" : "offline")}</div>
    <div class="muted mono" style="font-size:12px">${esc(n.ip || "")}${n.local ? " · MWM runs here" : ""}</div>
    ${[["CPU", (n.cpu || 0), 1, `${((n.cpu || 0) * 100).toFixed(0)}% of ${n.maxcpu} cores`], ["Memory", n.mem, n.maxmem, `${bytes(n.mem || 0)} / ${bytes(n.maxmem || 0)}`], ["Root disk", n.disk, n.maxdisk, `${bytes(n.disk || 0)} / ${bytes(n.maxdisk || 0)}`]].map(([l, a, b, t]) => `<div style="margin-top:10px"><div class="spread"><span class="label">${l}</span><span class="muted" style="font-size:12px">${t}</span></div><div class="bar ${pctv(a, b) > 90 ? "full" : ""}"><i style="width:${pctv(a, b)}%"></i></div></div>`).join("")}
    <div class="muted" style="margin-top:8px">Up ${upt(n.uptime)} · ${arr(PVE.ov.guests).filter((g) => g.node === n.node && g.status === "running").length} guests running</div>
    <div class="row" style="margin-top:12px"><button class="btn small primary" data-nview="${esc(n.local ? "" : n.node)}">Disks, SMART &amp; Docker</button><button class="btn small" data-deploy2="${esc(n.node)}">Deploy MWM</button><button class="btn small" data-ng="${esc(n.node)}">Guests</button></div></div>`).join("")}</div>`;
  $$("[data-nview]").forEach((b) => (b.onclick = () => { SRV.node = b.dataset.nview; SRV.info = null; go("server"); }));
  $$("[data-deploy2]").forEach((b) => (b.onclick = () => deployTo(b.dataset.deploy2)));
  $$("[data-ng]").forEach((b) => (b.onclick = () => { PVE.node = b.dataset.ng; PVE.tab = "guests"; drawCluster(); }));
}

// ----------------------------------------------------------- storage ----
function drawStorageTab() {
  const rows = arr(PVE.ov.storage).sort((a, b) => (a.storage + a.node).localeCompare(b.storage + b.node));
  $("#pv-body").innerHTML = `<div class="card" style="padding:0"><table class="table"><thead><tr><th>Storage</th><th>Node</th><th>Status</th><th>Type</th><th style="width:30%">Used</th><th class="num">Free</th></tr></thead><tbody>
    ${rows.map((s) => { const p = pctv(s.disk, s.maxdisk); return `<tr><td class="cell-main">${esc(s.storage)}${s.shared ? ' <span class="pill">shared</span>' : ""}<div class="cell-sub">${esc(s.content || "")}</div></td><td class="muted">${esc(s.node)}</td>
      <td>${s.status === "available" ? '<span class="pill low">available</span>' : `<span class="pill high">${esc(s.status || "unknown")}</span>`}</td><td class="muted">${esc(s.plugintype || "")}</td>
      <td>${s.maxdisk ? `<div class="bar ${p > 90 ? "full" : ""}"><i style="width:${p}%"></i></div><div class="cell-sub">${p}% of ${bytes(s.maxdisk)}</div>` : ""}</td><td class="num">${s.maxdisk ? bytes(s.maxdisk - s.disk) : ""}</td></tr>`; }).join("")}
    </tbody></table></div>
    ${rows.some((s) => s.status !== "available") ? '<div class="note warn-n" style="margin-top:12px">Storage that is not <b>available</b> can\'t be written to - backup jobs targeting it fail or silently skip.</div>' : ""}`;
}

// ----------------------------------------------------------- backups ----
function drawBackupsTab() {
  const o = PVE.ov;
  const guests = arr(o.guests).filter((g) => !g.template);
  const bad = guests.filter((g) => backupAge(g).cls !== "low").sort((a, b) => a.vmid - b.vmid);
  const jobs = arr(o.backup_jobs);
  const storeStatus = (name) => { const s = arr(o.storage).filter((x) => x.storage === name); return s.length ? (s.every((x) => x.status === "available") ? "available" : s.map((x) => `${x.node}: ${x.status}`).join(", ")) : "missing"; };
  $("#pv-body").innerHTML = `
    <div class="label" style="margin-bottom:8px">Scheduled backup jobs (${jobs.length})</div>
    <div class="card" style="padding:0"><table class="table"><thead><tr><th>Job</th><th>Schedule</th><th>Storage</th><th>Guests</th><th>Mode</th><th>Enabled</th></tr></thead><tbody>
    ${jobs.map((j) => { const ss = storeStatus(j.storage); return `<tr><td class="mono" style="font-size:12px">${esc(j.id || "")}<div class="cell-sub">${esc(j.comment || j["notes-template"] || "")}</div></td><td class="mono">${esc(j.schedule || j.starttime || "")}</td>
      <td>${esc(j.storage || "")} ${ss === "available" ? '<span class="pill low">ok</span>' : `<span class="pill high" title="${esc(ss)}">${ss === "missing" ? "missing" : "not available"}</span>`}</td>
      <td class="muted">${j.all ? "all" : esc(String(j.vmid || j.pool || ""))}</td><td class="muted">${esc(j.mode || "")}</td><td>${j.enabled === 0 ? '<span class="pill">off</span>' : '<span class="pill low">on</span>'}</td></tr>`; }).join("") || '<tr><td colspan="6" class="muted">No backup jobs defined.</td></tr>'}
    </tbody></table></div>
    <div class="label" style="margin:18px 0 8px">Guests without a backup in the last 7 days (${bad.length} of ${guests.length})</div>
    ${bad.length ? `<div class="note warn-n" style="margin-bottom:10px">Based on each node's backup task history. A guest listed here has no successful backup recently - if disaster struck, it could not be restored.</div>
      <div class="card" style="padding:0"><div class="scroll"><table class="table"><tbody>${bad.map((g) => { const b = backupAge(g); return `<tr data-guest="${esc(g.node)}|${esc(g.type)}|${g.vmid}" class="click"><td class="mono">${g.vmid}</td><td class="cell-main">${esc(g.name || "")}</td><td class="muted">${esc(g.node)}</td><td><span class="pill ${b.cls}">${esc(b.text)}</span></td>
        <td style="text-align:right"><button class="btn small primary" data-bknow="${esc(g.node)}|${esc(g.type)}|${g.vmid}|${esc(g.name || "")}">Back up now</button></td></tr>`; }).join("")}</tbody></table></div></div>`
      : '<div class="empty">✅<h3>Every guest has a recent backup</h3></div>'}`;
  $$("[data-bknow]").forEach((b) => (b.onclick = (e) => { e.stopPropagation(); const [n, t, v, nm] = b.dataset.bknow.split("|"); backupDialog(n, t, v, nm); }));
  $$("#pv-body [data-guest]").forEach((r) => (r.onclick = () => { const [n, t, v] = r.dataset.guest.split("|"); guestPanel(n, t, v); }));
}

// ------------------------------------------------------------- tasks ----
function drawTasksTab() {
  const t = arr(PVE.ov.tasks).slice().sort((a, b) => (b.starttime || 0) - (a.starttime || 0));
  $("#pv-body").innerHTML = `<div class="card" style="padding:0"><div class="scroll"><table class="table"><thead><tr><th>Started</th><th>Node</th><th>Task</th><th>Guest</th><th>User</th><th>Status</th></tr></thead><tbody>
    ${t.map((x) => `<tr class="click" data-ptask="${esc(x.node)}|${esc(x.upid)}"><td class="muted">${when(x.starttime)}</td><td>${esc(x.node)}</td><td class="mono">${esc(x.type)}</td><td class="mono">${esc(x.id || "")}</td><td class="muted">${esc(x.user || "")}</td>
      <td>${!x.status ? '<span class="pill medium">running</span>' : x.status === "OK" ? '<span class="pill low">OK</span>' : `<span class="pill high" title="${esc(x.status)}">${esc(String(x.status).slice(0, 40))}</span>`}</td></tr>`).join("")}
    </tbody></table></div></div>`;
  $$("[data-ptask]").forEach((r) => (r.onclick = async () => {
    const [node, upid] = r.dataset.ptask.split("|");
    modal(loading("Reading task log..."));
    const log = await guard(() => invoke("pve_task_log", { node, upid }));
    if (!log) return closeModal();
    modal(`<h2>Task log</h2><div class="muted mono" style="font-size:11.5px;word-break:break-all">${esc(upid)}</div><pre class="console mono" style="max-height:60vh">${esc(arr(log).join("\n"))}</pre><div class="row" style="justify-content:flex-end;margin-top:12px"><button class="btn" id="tl-copy">Copy</button><button class="btn primary" data-close>Close</button></div>`);
    $("#modal-card").classList.add("wide");
    $("#tl-copy").onclick = () => copyText(arr(log).join("\n"));
  }));
}

function drawHaTab() {
  const h = arr(PVE.ov.ha);
  $("#pv-body").innerHTML = `<div class="card" style="padding:0"><table class="table"><tbody>${h.map((x) => `<tr><td class="cell-main">${esc(x.id)}</td><td class="muted">${esc(x.type)}</td><td class="muted">${esc(x.node || "")}</td><td>${statePill(x.status || x.state || "")}</td><td class="muted">${esc(x.request_state || x.crm_state || "")}</td></tr>`).join("") || '<tr><td class="muted">HA is not configured.</td></tr>'}</tbody></table></div>`;
}

// ------------------------------------------------------ LXC updates ----
async function lxcOwnerCall(owner, cmd, args = {}) {
  return owner ? localInvoke("remote_call", { id: owner, cmd, args }) : localInvoke(cmd, args);
}

async function loadLxcUpdateReport() {
  await loadConns();
  const owners = [TARGET, ...FLEET.conns.map((c) => c.id).filter((id) => id !== TARGET)];
  let lastError;
  for (const owner of owners) {
    try {
      const report = await lxcOwnerCall(owner, "lxc_updates_report");
      PVE.updates = report;
      PVE.updateOwner = owner;
      PVE.updatePolicy = await lxcOwnerCall(owner, "lxc_updates_policy").catch(() => ({ auto_apply: [] }));
      return;
    } catch (e) { lastError = e; }
  }
  throw lastError || new Error("No Proxmox update scanner is connected");
}

async function drawLxcUpdatesTab() {
  const body = $("#pv-body");
  if (!PVE.updates) {
    body.innerHTML = loading("Loading scheduled container update inventory...");
    try { await loadLxcUpdateReport(); }
    catch (e) {
      if (current === "cluster" && PVE.tab === "lxc_updates")
        body.innerHTML = `<div class="empty"><h3>Update inventory unavailable</h3><p>${esc(String(e))}</p><p>The scheduled scanner runs on a Proxmox node and checks every LXC through the cluster.</p></div>`;
      return;
    }
  }
  if (current !== "cluster" || PVE.tab !== "lxc_updates") return;
  const rows = arr(PVE.updates.containers);
  const allowed = new Set(arr(PVE.updatePolicy?.auto_apply).map(String));
  const pending = rows.reduce((sum, row) => sum + (row.pending || 0), 0);
  const errors = rows.filter((row) => row.state === "error" || row.state === "stale").length;
  body.innerHTML = `
    <div class="grid g3">
      <div class="card"><div class="label">Containers checked</div><div class="v">${rows.length}</div><div class="muted">${rows.filter((r) => r.state === "stopped").length} stopped and left untouched</div></div>
      <div class="card"><div class="label">Package updates</div><div class="v">${num(pending)}</div><div class="muted">APT packages in running containers</div></div>
      <div class="card"><div class="label">Scan health</div><div class="v">${errors ? esc(errors + " need review") : "OK"}</div><div class="muted">${PVE.updates.generated_at ? esc(new Date(PVE.updates.generated_at).toLocaleString()) : ""}</div></div>
    </div>
    <div class="row" style="margin:14px 0;justify-content:space-between;flex-wrap:wrap"><span class="muted">Daily checks cover every LXC without an in-container agent. Auto updates run only for selected containers and take a Proxmox snapshot first.</span><button class="btn" id="lxc-update-scan">Scan now</button></div>
    <div class="card" style="padding:0"><div class="scroll"><table class="table"><thead><tr><th>CT</th><th>Container</th><th>Node</th><th>Status</th><th class="num">Updates</th><th>Automatic</th><th></th></tr></thead><tbody>
      ${rows.map((row) => `<tr><td class="mono">${esc(row.vmid)}</td><td class="cell-main">${esc(row.name)}<div class="cell-sub">${esc(arr(row.packages).slice(0, 8).join(", "))}</div></td><td class="muted">${esc(row.node)}</td><td>${statePill(row.state)}</td><td class="num">${num(row.pending)}</td>
        <td><input type="checkbox" class="cb" data-lxc-auto="${esc(row.vmid)}" ${allowed.has(String(row.vmid)) ? "checked" : ""} ${row.manager !== "apt" ? "disabled" : ""}></td>
        <td><button class="btn small" data-lxc-apply="${esc(row.vmid)}" ${row.state !== "ok" || !row.pending ? "disabled" : ""}>Update now</button></td></tr>`).join("")}
    </tbody></table></div></div>`;
  $("#lxc-update-scan").onclick = async () => {
    const result = await guard(() => lxcOwnerCall(PVE.updateOwner, "lxc_updates_scan"));
    if (result?.job) { toast("Cluster update scan started."); followLxcUpdateJob(result.job, PVE.updateOwner); }
  };
  $$("[data-lxc-auto]").forEach((box) => (box.onchange = async () => {
    const id = box.dataset.lxcAuto;
    if (box.checked && !(await choose(`Automatically update CT ${esc(id)}?`,
      "<p class='muted'>MWM will take a Proxmox snapshot, then install available APT updates during the daily maintenance run. Service restarts inside this container may occur.</p>",
      [["go", "Enable", "primary"]]))) { box.checked = false; return; }
    const ids = new Set(allowed);
    if (box.checked) ids.add(id); else ids.delete(id);
    const value = await guard(() => lxcOwnerCall(PVE.updateOwner, "lxc_updates_policy_set", { ids: [...ids] }));
    if (!value) { box.checked = !box.checked; return; }
    PVE.updatePolicy = value;
    toast(`Automatic updates ${box.checked ? "enabled" : "disabled"} for CT ${id}.`);
    drawLxcUpdatesTab();
  }));
  $$("[data-lxc-apply]").forEach((button) => (button.onclick = async () => {
    const row = rows.find((item) => String(item.vmid) === button.dataset.lxcApply);
    if (!row) return;
    if (!(await choose(`Update ${esc(row.name)} (CT ${esc(row.vmid)})?`,
      "<p class='muted'>MWM first takes a Proxmox snapshot, then runs APT upgrade in the container. The snapshot is kept for recovery.</p>",
      [["go", "Update", "primary"]]))) return;
    const result = await guard(() => lxcOwnerCall(PVE.updateOwner, "lxc_updates_apply", { node: row.node, vmid: String(row.vmid) }));
    if (result?.job) { toast(`CT ${row.vmid} update started.`); followLxcUpdateJob(result.job, PVE.updateOwner); }
  }));
}

async function followLxcUpdateJob(id, owner) {
  for (let n = 0; n < 900; n++) {
    await new Promise((resolve) => setTimeout(resolve, 2000));
    const jobs = await lxcOwnerCall(owner, "jobs_list").catch(() => []);
    const job = arr(jobs).find((item) => item.id === id);
    if (job && job.state !== "running") {
      toast(`${job.title}: ${job.message || job.state}`, job.state === "failed");
      PVE.updates = null;
      if (current === "cluster" && PVE.tab === "lxc_updates") drawLxcUpdatesTab();
      return;
    }
  }
  toast("Update job is still running; refresh to check its status.");
}
