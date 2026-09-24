/* MWM - Keys & Licenses vault and the Server / NAS page (Proxmox, Unraid,
   ZFS, SMART, Docker). */
"use strict";

// ---------------------------------------------------------------- Keys ----
const KEYS = { list: null, shown: new Set() };
const KIND_LABEL = {
  windows: "Windows", office: "Microsoft Office", license: "Other licenses", bitlocker: "BitLocker recovery",
  wifi: "Wi-Fi networks", ssh: "SSH host keys", proxmox: "Proxmox", unraid: "Unraid", wireguard: "WireGuard",
};

VIEWS.keys = async function () {
  $("#top-actions").innerHTML = `<button class="btn" id="k-re">Refresh</button><button class="btn" id="k-all">Show all</button><button class="btn" id="k-copy">Copy all</button><button class="btn primary" id="k-exp">${icon("key")}Export keys</button>`;
  $("#k-re").onclick = () => { KEYS.list = null; VIEWS.keys(); };
  if (!KEYS.list) {
    $("#page").innerHTML = loading("Reading product keys, licenses, recovery keys and saved networks...");
    const l = await guard(() => invoke("keys_list"));
    if (!l) return;
    KEYS.list = l;
    if (current !== "keys") return;
  }
  drawKeys();
  $("#k-all").onclick = () => {
    const all = KEYS.shown.size < KEYS.list.length;
    KEYS.shown = new Set(all ? KEYS.list.map((_, i) => i) : []);
    $("#k-all").textContent = all ? "Hide all" : "Show all";
    drawKeys();
  };
  $("#k-copy").onclick = () => copyText(keysText());
  $("#k-exp").onclick = async () => {
    const ok = await choose("Export keys?", `<p class="muted">The file contains product keys, recovery keys and Wi-Fi passwords <b>in plain text</b>. Store it somewhere safe (a password manager or an encrypted USB stick) and delete stray copies.</p>`, [["go", "Export", "primary"]]);
    if (!ok) return;
    const name = `MWM-Keys-${S.info?.hostname || "PC"}-${new Date().toISOString().slice(0, 10)}.txt`;
    if (await guard(() => saveText(name, keysText(), "txt"))) toast("Keys exported.");
  };
};

function keysText() {
  const lines = [`MWM - Move Weight Manager - keys for ${S.info?.hostname || ""} (${new Date().toLocaleString()})`, ""];
  for (const kind of Object.keys(KIND_LABEL)) {
    const items = KEYS.list.filter((k) => k.kind === kind && k.value);
    if (!items.length) continue;
    lines.push(`== ${KIND_LABEL[kind]} ==`);
    items.forEach((k) => lines.push(`${k.name}: ${k.value}${k.source ? `   [${k.source}]` : ""}${k.note ? `\n    ${k.note}` : ""}`));
    lines.push("");
  }
  return lines.join("\n");
}

function drawKeys() {
  const kinds = Object.keys(KIND_LABEL).filter((k) => KEYS.list.some((x) => x.kind === k));
  const mask = (v) => v.replace(/[^-\s]/g, "•");
  $("#page").innerHTML = `
    <p class="muted" style="margin-top:0">Everything you'd want to pull off this machine before a reinstall or a repair. Keys stay on this computer - MWM never sends them anywhere.</p>
    ${kinds.length ? kinds.map((kind) => `<div class="label" style="margin:18px 0 8px">${esc(KIND_LABEL[kind])}</div>
      <div class="card" style="padding:0"><table class="table"><tbody>${KEYS.list.map((k, i) => [k, i]).filter(([k]) => k.kind === kind).map(([k, i]) => `
        <tr><td style="width:34%"><div class="cell-main">${esc(k.name)}</div><div class="cell-sub">${esc(k.source)}</div></td>
          <td><div class="mono keyval">${k.value ? esc(k.secret && !KEYS.shown.has(i) ? mask(k.value) : k.value) : '<span class="faint">-</span>'}</div>${k.note ? `<div class="cell-sub">${esc(k.note)}</div>` : ""}</td>
          <td style="width:150px;text-align:right">${k.value ? `${k.secret ? `<button class="btn small ghost" data-kshow="${i}">${KEYS.shown.has(i) ? "Hide" : "Show"}</button>` : ""}<button class="btn small" data-kcopy="${i}">Copy</button>` : ""}</td></tr>`).join("")}
      </tbody></table></div>`).join("") : `<div class="empty">${icon("key")}<h3>No keys found</h3>Nothing readable on this machine.</div>`}
    ${S.info?.elevated ? "" : adminNote(true).replace("Some items need administrator rights.", "BitLocker recovery keys and some Wi-Fi passwords need administrator rights.")}`;
  $$("[data-kshow]").forEach((b) => (b.onclick = () => { const i = +b.dataset.kshow; KEYS.shown.has(i) ? KEYS.shown.delete(i) : KEYS.shown.add(i); drawKeys(); }));
  $$("[data-kcopy]").forEach((b) => (b.onclick = () => copyText(KEYS.list[+b.dataset.kcopy].value)));
}

// -------------------------------------------------------------- Server ----
const SRV = { info: null };
const pct = (a, b) => (b ? Math.round((a / b) * 100) : 0);
const upt = (s) => (!s ? "" : s > 86400 ? `${Math.floor(s / 86400)}d ${Math.floor((s % 86400) / 3600)}h` : `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`);
const statePill = (s) => `<span class="pill ${/^(running|online|active|started|ONLINE|healthy|up)/i.test(s) ? "low" : /^(stopped|exited|created|paused)/i.test(s) ? "" : "high"}">${esc(s)}</span>`;

VIEWS.server = async function () {
  $("#top-actions").innerHTML = `<button class="btn" id="sv-re">Refresh</button>`;
  $("#sv-re").onclick = () => { SRV.info = null; VIEWS.server(); };
  if (!SRV.info) {
    $("#page").innerHTML = loading("Reading guests, pools, disks and containers...");
    const i = await guard(() => invoke("server_info", { node: SRV.node || "" }));
    if (!i) return;
    SRV.info = i;
    if (current !== "server") return;
  }
  drawServer();
};

async function srvAction(action, target, label) {
  const ok = await choose(esc(label), `<p class="muted mono">${esc(action)} ${esc(target)}</p>`, [["go", "Do it", "primary"]]);
  if (!ok) return;
  const r = await guard(() => invoke("server_action", { action, target }));
  if (r) { toast(String(r).slice(0, 200)); SRV.info = null; setTimeout(() => current === "server" && VIEWS.server(), 1500); }
}

function drawServer() {
  const i = SRV.info;
  const pve = i.proxmox, un = i.unraid, dk = i.docker;
  const zfs = arr(i.zfs), smart = arr(i.smart);
  const platName = { proxmox: "Proxmox VE", unraid: "Unraid", truenas: "TrueNAS", linux: "Linux", windows: "Windows", macos: "macOS" }[i.platform] || i.platform;
  const nothing = !pve && !un && !zfs.length && !smart.length && !dk;
  const guests = pve ? arr(pve.guests).sort((a, b) => a.vmid - b.vmid) : [];
  $("#page").innerHTML = `
    <div class="grid g4">
      <div class="card"><div class="label">Platform</div><div class="v" style="font-size:20px;font-weight:700;margin:6px 0">${esc(platName)}</div><div class="muted mono" style="font-size:12px">${esc(pve?.version || un?.version || i.kernel || "")}</div></div>
      <div class="card"><div class="label">Load (1/5/15)</div><div class="v" style="font-size:20px;font-weight:700;margin:6px 0">${esc(i.load || "-")}</div><div class="muted">${S.info?.cores || ""} threads</div></div>
      <div class="card"><div class="label">Failed services</div><div class="v ${arr(i.failed).length ? "danger-t" : "accent"}" style="font-size:20px;font-weight:700;margin:6px 0">${arr(i.failed).length}</div><div class="muted">${arr(i.failed).map((f) => `<span class="mono">${esc(f)}</span> <a href="#" data-srv="service_restart|${esc(f)}|Restart ${esc(f)}">restart</a>`).join("<br>") || "all good"}</div></div>
      <div class="card"><div class="label">Kernels installed</div><div class="v" style="font-size:20px;font-weight:700;margin:6px 0">${arr(i.kernels).length || "-"}</div><div class="muted">${arr(i.kernels).length > 2 ? `<a href="#" data-task="kernels">remove old kernels</a>` : `running ${esc(i.kernel)}`}</div></div>
    </div>
    ${nothing && i.platform === "windows" ? `<div class="card" style="margin-top:14px"><h3 style="margin-top:0">Manage your servers from here</h3><p class="muted">This page lights up on Proxmox, Unraid and other Linux boxes. Install MWM there and open it in any browser:</p>
      <pre class="console mono">curl -fsSL https://github.com/BlizzHacker/mwm/releases/latest/download/install.sh | sh -s -- --web</pre><p class="muted">Then browse to <span class="mono">http://your-server:7777</span> and paste the access token it prints.</p></div>` : ""}
    ${pve ? `<div class="label" style="margin:18px 0 8px">Proxmox guests ${pve.subscription ? `<span class="pill ${/active/i.test(pve.subscription) ? "low" : ""}">${/notfound/i.test(pve.subscription) ? "no subscription" : esc(pve.subscription)}</span>` : ""}</div>
      <div class="card" style="padding:0"><div class="scroll" style="max-height:44vh"><table class="table"><thead><tr><th>ID</th><th>Name</th><th>Type</th><th>Status</th><th class="num">CPU</th><th class="num">Memory</th><th class="num">Uptime</th><th></th></tr></thead><tbody>
      ${guests.map((g) => { const t = `${g.node}/${g.type}/${g.vmid}`; return `<tr><td class="mono">${g.vmid}</td><td class="cell-main">${esc(g.name || "")}<div class="cell-sub">${esc(g.node || "")}</div></td><td class="muted">${g.type === "lxc" ? "container" : "VM"}</td><td>${statePill(g.status || "")}</td>
        <td class="num">${g.cpu != null ? (g.cpu * 100).toFixed(0) + "%" : ""}</td><td class="num">${g.maxmem ? `${bytes(g.mem || 0)} / ${bytes(g.maxmem)}` : ""}</td><td class="num muted">${upt(g.uptime)}</td>
        <td style="white-space:nowrap;text-align:right">${g.status === "running" ? `<button class="btn small" data-srv="guest_reboot|${t}|Reboot ${esc(g.name)}">Reboot</button> <button class="btn small danger" data-srv="guest_shutdown|${t}|Shut down ${esc(g.name)}">Shut down</button>` : `<button class="btn small primary" data-srv="guest_start|${t}|Start ${esc(g.name)}">Start</button>`}</td></tr>`; }).join("")}
      </tbody></table></div></div>
      <div class="label" style="margin:18px 0 8px">Proxmox storage</div><div class="card" style="padding:0"><table class="table"><tbody>
      ${arr(pve.storage).map((s) => `<tr><td class="cell-main">${esc(s.name)}<div class="cell-sub">${esc(s.type)}</div></td><td>${statePill(s.status)}</td><td style="width:40%">${s.total ? `<div class="bar ${pct(s.used, s.total) > 90 ? "full" : ""}"><i style="width:${pct(s.used, s.total)}%"></i></div>` : ""}</td><td class="num">${s.total ? `${bytes(s.used)} / ${bytes(s.total)}` : ""}</td></tr>`).join("")}
      </tbody></table></div>` : ""}
    ${un ? `<div class="label" style="margin:18px 0 8px">Unraid array <span class="pill ${un.array === "STARTED" ? "low" : "high"}">${esc(un.array || "unknown")}</span></div>
      <div class="card" style="padding:0"><table class="table"><thead><tr><th>Disk</th><th>Device</th><th>Status</th><th class="num">Temp</th><th class="num">Used</th><th class="num">Errors</th></tr></thead><tbody>
      ${arr(un.disks).map((d) => `<tr><td class="cell-main">${esc(d.name)}</td><td class="mono muted">${esc(d.device)} ${esc(d.fs)}</td><td>${statePill(d.status.replace("DISK_", "").toLowerCase() === "ok" ? "healthy" : d.status)}</td><td class="num">${d.temp && d.temp !== "*" ? d.temp + "°C" : "spun down"}</td><td class="num">${d.size ? `${pct(d.size - d.free, d.size)}%` : ""}</td><td class="num">${esc(d.errors)}</td></tr>`).join("")}</tbody></table></div>` : ""}
    ${zfs.length ? `<div class="label" style="margin:18px 0 8px">ZFS pools</div><div class="grid g2">${zfs.map((z) => `<div class="card"><div class="spread"><b>${esc(z.name)}</b>${statePill(z.health)}</div>
      <div class="bar ${pct(z.alloc, z.size) > 85 ? "full" : ""}" style="margin:10px 0 6px"><i style="width:${pct(z.alloc, z.size)}%"></i></div>
      <div class="spread muted"><span>${bytes(z.alloc)} of ${bytes(z.size)} (${esc(z.cap)}%)</span><span>frag ${esc(z.frag)}%</span></div>
      <div class="cell-sub" style="margin-top:8px">${esc(z.scan || "never scrubbed")}</div><div class="cell-sub">errors: ${esc(z.errors)}</div>
      <button class="btn small" style="margin-top:10px" data-srv="zfs_scrub|${esc(z.name)}|Scrub pool ${esc(z.name)}">Scrub now</button></div>`).join("")}</div>` : ""}
    ${smart.length ? `<div class="label" style="margin:18px 0 8px">Disk health (SMART)</div><div class="card" style="padding:0"><table class="table"><thead><tr><th>Disk</th><th>Health</th><th class="num">Temp</th><th class="num">Power-on</th><th class="num">Wear</th><th class="num">Bad sectors</th><th></th></tr></thead><tbody>
      ${smart.map((d) => { const bad = (d.reallocated || 0) + (d.pending || 0) + (d.media_errors || 0); return `<tr><td class="cell-main">${esc(d.model)}<div class="cell-sub mono">${esc(d.device)}${d.capacity ? " - " + bytes(d.capacity) : ""}${d.rotation ? " - HDD" : " - SSD"}</div></td>
        <td>${d.passed === null || d.passed === undefined ? '<span class="pill">unknown</span>' : okPill(d.passed, "passed", "FAILING")}</td><td class="num ${d.temp > 55 ? "warn" : ""}">${d.temp != null ? d.temp + "°C" : "-"}</td><td class="num">${d.hours != null ? num(d.hours) + " h" : "-"}</td><td class="num">${d.wear != null ? d.wear + "%" : "-"}</td><td class="num ${bad ? "danger-t" : ""}">${bad}</td>
        <td><button class="btn small" data-srv="smart_test|${esc(d.device)}|Start a short SMART self-test on ${esc(d.device)}">Self-test</button></td></tr>`; }).join("")}</tbody></table></div>` : ""}
    ${dk ? `<div class="spread" style="margin:18px 0 8px"><div class="label">Docker containers</div><button class="btn small" data-task="dockerprune">Clean up Docker</button></div>
      <div class="card" style="padding:0"><table class="table"><tbody>${arr(dk.containers).map((c) => `<tr><td class="cell-main">${esc(c.name)}<div class="cell-sub mono">${esc(c.image)}</div></td><td>${statePill(c.state)}</td><td class="muted">${esc(c.status)}</td>
        <td style="text-align:right;white-space:nowrap">${c.state === "running" ? `<button class="btn small" data-srv="docker_restart|${esc(c.name)}|Restart ${esc(c.name)}">Restart</button> <button class="btn small danger" data-srv="docker_stop|${esc(c.name)}|Stop ${esc(c.name)}">Stop</button>` : `<button class="btn small primary" data-srv="docker_start|${esc(c.name)}|Start ${esc(c.name)}">Start</button>`}</td></tr>`).join("") || '<tr><td class="muted">No containers.</td></tr>'}</tbody></table>
        ${arr(dk.df).length ? `<div class="row" style="padding:10px 14px;flex-wrap:wrap">${arr(dk.df).map((d) => `<span class="pill">${esc(d.type)}: ${esc(d.size)} (reclaimable ${esc(d.reclaimable)})</span>`).join("")}</div>` : ""}</div>` : ""}`;
  $$("[data-srv]").forEach((b) => (b.onclick = (e) => { e.preventDefault(); const [a, t, l] = b.dataset.srv.split("|"); srvAction(a, t, l); }));
}

// --------------------------------------------- Docker inside LXC guests ----
// Pick a container on the Server page and see / clean the Docker inside it.
// Volumes are never touched - that's where container data lives.
SRV.dockerScan = null;
SRV.dockerJob = null;

async function dockerPanel(node, vmid, name) {
  modal(loading(`Reading Docker inside CT ${vmid} ${name}...`));
  $("#modal-card").classList.add("wide");
  const d = await guard(() => invoke("guest_docker", { node, vmid: String(vmid) }));
  if (!d) return closeModal();
  if (!d.available) {
    modal(`<h2>CT ${esc(vmid)} ${esc(name)}</h2><p class="muted">Docker isn't installed in this container.</p><div class="row" style="justify-content:flex-end"><button class="btn primary" data-close>Close</button></div>`);
    return;
  }
  const cs = arr(d.containers);
  const stopped = cs.filter((c) => c.state !== "running").length;
  modal(`<div class="spread"><h2 style="margin:0">Docker in CT ${esc(vmid)} · ${esc(name)}</h2><span class="muted">${esc(node)}</span></div>
    <div class="row" style="flex-wrap:wrap;margin:12px 0">${arr(d.df).map((x) => `<span class="pill">${esc(x.type)}: ${esc(x.size)} · reclaimable ${esc(x.reclaimable)}</span>`).join("")}
      <span class="pill ${d.dangling_images ? "medium" : ""}">${d.dangling_images} dangling image(s)</span></div>
    <div class="scroll" style="max-height:42vh"><table class="table"><thead><tr><th>Container</th><th>State</th><th>Status</th><th class="num">Size</th><th></th></tr></thead><tbody>
    ${cs.map((c) => `<tr><td class="cell-main">${esc(c.name)}<div class="cell-sub mono">${esc(c.image)}</div></td><td>${statePill(c.state)}</td><td class="muted">${esc(c.status)}</td><td class="num muted">${esc(c.size || "")}</td>
      <td style="white-space:nowrap;text-align:right">${c.state === "running"
        ? `<button class="btn small" data-dk="restart|${esc(c.name)}">Restart</button> <button class="btn small danger" data-dk="stop|${esc(c.name)}">Stop</button>`
        : `<button class="btn small primary" data-dk="start|${esc(c.name)}">Start</button> <button class="btn small danger" data-dk="remove|${esc(c.name)}">Remove</button>`}</td></tr>`).join("") || '<tr><td colspan="5" class="muted">No containers.</td></tr>'}
    </tbody></table></div>
    <div class="label" style="margin:16px 0 8px">Clean up</div>
    <div class="row" style="flex-wrap:wrap">
      <button class="btn primary" data-dk="cleanup|">Safe cleanup</button>
      <button class="btn" data-dk="prune_containers|">Remove ${stopped} stopped container(s)</button>
      <button class="btn" data-dk="prune_images|">Remove dangling images</button>
      <button class="btn" data-dk="prune_builder|">Clear build cache</button>
      <button class="btn danger" data-dk="prune_images_unused|">Remove ALL unused images</button>
    </div>
    <div class="cell-sub" style="margin-top:8px">Safe cleanup = stopped containers + dangling images + build cache + unused networks. Volumes (your container data) are never removed.</div>
    <div id="dk-msg" style="margin-top:10px"></div>
    <div class="row" style="justify-content:flex-end;margin-top:12px"><button class="btn" data-close>Close</button></div>`);
  $("#modal-card").classList.add("wide");
  $$("[data-dk]").forEach((b) => (b.onclick = async () => {
    const [action, target] = b.dataset.dk.split("|");
    const risky = { remove: `Remove container ${target}? Its volumes stay.`, prune_containers: `Remove ${stopped} stopped container(s)?`, prune_images_unused: "Remove every image no container uses? They'll be re-downloaded if needed later.", cleanup: "Run the safe cleanup?", stop: `Stop ${target}?` }[action];
    if (risky && !(await choose("Are you sure?", `<p class="muted">${esc(risky)}</p>`, [["go", "Yes, do it", action.startsWith("prune_images_unused") || action === "remove" ? "danger" : "primary"]]))) return dockerPanel(node, vmid, name);
    if (risky) modal(loading("Working inside the container..."));
    else $("#dk-msg").innerHTML = '<span class="spin sm"></span> Working...';
    const r = await guard(() => invoke("guest_docker_action", { node, vmid: String(vmid), action, target }));
    if (r !== undefined) { toast(String(r).slice(0, 220) || "Done"); SRV.dockerScan = null; dockerPanel(node, vmid, name); }
    else $("#dk-msg").innerHTML = "";
  }));
}

function drawDockerScan() {
  const box = $("#dk-scan");
  if (!box) return;
  const j = SRV.dockerJob && JOBS.list.find((x) => x.id === SRV.dockerJob);
  const rows = (j ? j.output : SRV.dockerScan || []).map((l) => { try { return typeof l === "string" ? JSON.parse(l) : l; } catch { return null; } }).filter(Boolean);
  if (j && j.state !== "running") SRV.dockerScan = rows;
  const reclaim = (df) => df.split(";").map((x) => x.split("|")).filter((x) => x.length === 3).map((x) => `${x[0]}: ${x[2]}`).join(" · ");
  box.innerHTML = `${j && j.state === "running" ? `<div class="muted" style="margin-bottom:8px"><span class="spin sm"></span> Checking ${esc(j.current)} (${j.done_items}/${j.total_items})</div>` : ""}
    ${rows.length ? `<div class="card" style="padding:0"><table class="table"><thead><tr><th>LXC</th><th>Node</th><th class="num">Containers</th><th>Reclaimable</th><th></th></tr></thead><tbody>
    ${rows.map((r) => `<tr><td class="cell-main">${esc(r.vmid)} ${esc(r.name)}</td><td class="muted">${esc(r.node)}</td><td class="num">${esc(r.running)} running / ${esc(r.total)}</td><td class="muted" style="font-size:12.5px">${esc(reclaim(r.df))}</td>
      <td style="text-align:right"><button class="btn small primary" data-dkopen="${esc(r.node)}|${esc(r.vmid)}|${esc(r.name)}">Open</button></td></tr>`).join("")}</tbody></table></div>`
      : j && j.state === "running" ? "" : SRV.dockerScan ? '<div class="muted">No running LXC has Docker.</div>' : ""}`;
  $$("[data-dkopen]").forEach((b) => (b.onclick = () => { const [n, v, nm] = b.dataset.dkopen.split("|"); dockerPanel(n, v, nm); }));
}
document.addEventListener("mdw-jobs", () => { if (current === "server") drawDockerScan(); });

// Hook into the Server page: Docker buttons on LXC rows + the cluster scan.
const _drawServer = drawServer;
drawServer = function () {
  _drawServer();
  const pve = SRV.info?.proxmox;
  if (!pve) return;
  const label = [...$$("#page .label")].find((l) => l.textContent.startsWith("Proxmox guests"));
  if (label) label.insertAdjacentHTML("beforebegin", `<div class="spread" style="margin:18px 0 8px"><div class="label">Docker in your LXCs</div><button class="btn small primary" id="dk-scan-go">Find Docker in all LXCs</button></div><div id="dk-scan"></div>`);
  $("#dk-scan-go") && ($("#dk-scan-go").onclick = async () => {
    const id = await guard(() => invoke("docker_scan_cluster"));
    if (!id) return;
    SRV.dockerJob = id;
    watchJob(id, () => drawDockerScan(), true);
    drawDockerScan();
  });
  drawDockerScan();
  // A Docker button on every running container row.
  $$("#page [data-srv^='guest_reboot|']").forEach((b) => {
    const [, t] = b.dataset.srv.split("|");
    const [node, type, vmid] = t.split("/");
    if (type !== "lxc") return;
    const name = arr(pve.guests).find((g) => String(g.vmid) === vmid)?.name || "";
    b.insertAdjacentHTML("beforebegin", `<button class="btn small" data-dkct="${esc(node)}|${esc(vmid)}|${esc(name)}">Docker</button> `);
  });
  $$("[data-dkct]").forEach((b) => (b.onclick = () => { const [n, v, nm] = b.dataset.dkct.split("|"); dockerPanel(n, v, nm); }));
};


// ------------------------------------------- all Proxmox nodes + mounts ----
SRV.node = "";
SRV.nodes = null;

async function deployTo(node) {
  const ok = await choose(`Install MWM on ${esc(node)}?`, `<p class="muted">Copies MWM to <b>${esc(node)}</b>, starts its web server on port 7777 (with its own access token) and adds it to your Machines - then Commander, cleanup and repairs work on that node too.</p>`, [["go", "Install", "primary"]]);
  if (!ok) return;
  modal(loading(`Installing MWM on ${node}...`));
  const r = await guard(() => invoke("deploy_node", { node }));
  closeModal();
  if (!r) return;
  if (!WEB || TAURI) {
    await guard(() => localInvoke("conn_save", { id: null, name: `${node} (Proxmox)`, url: r.link, token: "" }));
    if (typeof renderSwitcher === "function") renderSwitcher();
    toast(`MWM is running on ${node} and was added to Machines.`);
  } else {
    modal(`<h2>MWM is running on ${esc(node)}</h2><p class="muted">Add it on your desktop MWM (Machines → Add machine) with this link:</p><div class="mono keyval">${esc(r.link)}</div><div class="row" style="justify-content:flex-end;margin-top:14px"><button class="btn" id="dp-copy">Copy</button><button class="btn primary" data-close>Done</button></div>`);
    $("#dp-copy").onclick = () => copyText(r.link);
  }
}

const _drawServer2 = drawServer;
drawServer = function () {
  _drawServer2();
  const i = SRV.info;
  if (i.nodes) SRV.nodes = i.nodes;
  const page = $("#page");
  const nodes = arr(SRV.nodes);
  const pctOf = (a, b) => (b ? Math.round((a / b) * 100) : 0);
  const head = nodes.length ? `<div class="label" style="margin-bottom:8px">Proxmox cluster · click a node to inspect it</div>
    <div class="grid g3" style="margin-bottom:14px">${nodes.map((n) => {
      const on = (SRV.node || nodes.find((x) => x.local)?.node) === n.node;
      return `<div class="card mcard" data-node="${esc(n.local ? "" : n.node)}" style="cursor:pointer;${on ? "border-color:var(--accent)" : ""}">
        <div class="spread"><b>${esc(n.node)}</b>${statePill(n.status === "online" ? "online" : "offline")}</div>
        <div class="muted mono" style="font-size:12px">${esc(n.ip || "")}${n.local ? " · this node" : ""}</div>
        <div class="grid g3" style="gap:8px;margin-top:8px"><div><div class="label">CPU</div><div class="v">${Math.round((n.cpu || 0) * 100)}%</div></div><div><div class="label">RAM</div><div class="v">${pctOf(n.mem, n.maxmem)}%</div></div><div><div class="label">Up</div><div class="v">${upt(n.uptime)}</div></div></div>
        <div class="row" style="margin-top:10px"><button class="btn small" data-deploy="${esc(n.node)}">Deploy MWM</button></div></div>`;
    }).join("")}</div>` : "";
  const mounts = arr(i.mounts);
  const mountsHtml = mounts.length ? `<div class="label" style="margin:18px 0 8px">Disks &amp; mounts on ${esc(i.node || "this machine")}</div>
    <div class="card" style="padding:0"><table class="table"><tbody>${mounts.map((m) => { const p = pctOf(m.used, m.total); return `<tr><td class="cell-main mono">${esc(m.mount)}<div class="cell-sub">${esc(m.source)} · ${esc(m.fs)}</div></td>
      <td style="width:40%"><div class="bar ${p > 93 ? "full" : ""}"><i style="width:${p}%"></i></div></td><td class="num">${bytes(m.free)} free of ${bytes(m.total)}</td><td class="num ${p > 93 ? "danger-t" : "muted"}">${p}%</td></tr>`; }).join("")}</tbody></table></div>` : "";
  page.insertAdjacentHTML("afterbegin", head);
  // Mounts go right after the four summary cards.
  const firstGrid = page.querySelector(".grid.g4");
  if (firstGrid) firstGrid.insertAdjacentHTML("afterend", mountsHtml);
  $$("[data-node]").forEach((c) => (c.onclick = (e) => {
    if (e.target.closest("[data-deploy]")) return;
    SRV.node = c.dataset.node;
    SRV.info = null;
    SRV.dockerScan = null;
    VIEWS.server();
  }));
  $$("[data-deploy]").forEach((b) => (b.onclick = (e) => { e.stopPropagation(); deployTo(b.dataset.deploy); }));
};
