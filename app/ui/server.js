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
    const i = await guard(() => invoke("server_info"));
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
