/* MWM - Machines: manage every MWM install (Proxmox, Unraid, Linux, other PCs)
   from this one. Pick a machine in the sidebar switcher and every page drives it. */
"use strict";

const FLEET = { conns: [], status: {}, open: false };

async function loadConns() {
  FLEET.conns = (await localInvoke("conn_list").catch(() => [])) || [];
  if (TARGET && !FLEET.conns.some((c) => c.id === TARGET)) setTargetRaw("");
}
function setTargetRaw(id) {
  TARGET = id;
  try { localStorage.setItem("mwm.target", id); } catch {}
}
const targetConn = () => FLEET.conns.find((c) => c.id === TARGET);

async function renderSwitcher() {
  await loadConns();
  const box = $("#switcher");
  if (!box) return;
  const c = targetConn();
  box.innerHTML = `<button class="sw ${c ? "remote" : ""}" id="sw-btn"><span class="dot"></span><span class="sw-t">${c ? esc(c.name) : "This PC"}<small>${c ? esc(c.url.replace(/^https?:\/\//, "")) : esc(S.info?.hostname || "local")}</small></span>▾</button>
    ${FLEET.open ? `<div class="sw-menu"><div class="sw-item ${!c ? "on" : ""}" data-sw="">🖥 This PC</div>${FLEET.conns.map((x) => `<div class="sw-item ${x.id === TARGET ? "on" : ""}" data-sw="${esc(x.id)}">🗄 ${esc(x.name)}</div>`).join("")}<div class="sw-item" data-go="fleet">＋ Manage machines...</div></div>` : ""}`;
  $("#sw-btn").onclick = () => { FLEET.open = !FLEET.open; renderSwitcher(); };
  $$("[data-sw]").forEach((el) => (el.onclick = () => switchTo(el.dataset.sw)));
  $$(".sw-menu [data-go]").forEach((el) => el.addEventListener("click", () => { FLEET.open = false; renderSwitcher(); }));
}

/** Point the whole UI at another machine and reload what's on screen. */
async function switchTo(id) {
  FLEET.open = false;
  setTargetRaw(id);
  // Forget everything cached from the previous machine.
  Object.assign(S, { scan: null, apps: null, startup: null, updates: null, lastClean: null, checked: new Set() });
  if (typeof TK !== "undefined") Object.assign(TK, { report: null, security: null, events: null, network: null, wifi: null, tasks: null, taskJobs: {} });
  if (typeof SRV !== "undefined") SRV.info = null;
  if (typeof KEYS !== "undefined") { KEYS.list = null; KEYS.shown = new Set(); }
  if (typeof CMD !== "undefined") { CMD.roots = []; CMD.panes.forEach((p) => { p.listing = null; p.path = ""; p.rows = []; p.sel.clear(); }); }
  JOBS.list = [];
  try { S.info = await invoke("system_info"); } catch (e) { toast(String(e), true); setTargetRaw(""); S.info = await invoke("system_info"); }
  $("#elev").className = "elev" + (S.info.elevated ? " admin" : "");
  $("#elev").innerHTML = `<span class="dot"></span>${S.info.elevated ? "Administrator" : `Standard user - <a href="#" data-admin>elevate</a>`}`;
  $("#ver").textContent = `v${S.info.version || ""} - ${S.info.hostname || ""}`;
  await renderSwitcher();
  pollJobs();
  toast(TARGET ? `Now managing ${targetConn()?.name} (${S.info.hostname}).` : "Back on this PC.");
  go(current === "fleet" ? (S.info.platform === "windows" ? "health" : "server") : current);
}

// Remote banner in the top bar on every page.
document.addEventListener("mdw-page", () => {});
const _go = go;
go = function (id) {
  _go(id);
  const c = targetConn();
  if (c && id !== "fleet") $("#top-actions").insertAdjacentHTML("afterbegin", `<span class="remote-banner">Managing ${esc(c.name)} · ${esc(S.info?.hostname || "")}</span>`);
};

// ---------------------------------------------------------------- page ----
VIEWS.fleet = async function () {
  $("#top-actions").innerHTML = `<button class="btn" id="fl-re">Refresh</button><button class="btn primary" id="fl-add">＋ Add machine</button>`;
  $("#fl-add").onclick = () => editConn();
  $("#fl-re").onclick = () => { FLEET.status = {}; VIEWS.fleet(); };
  await loadConns();
  drawFleet();
  // Probe every machine in parallel; cards fill in as answers arrive.
  for (const c of FLEET.conns) {
    if (FLEET.status[c.id]) continue;
    FLEET.status[c.id] = { loading: true };
    probe(c).then((st) => { FLEET.status[c.id] = st; if (current === "fleet") drawFleet(); });
  }
};

async function probe(c) {
  const t0 = Date.now();
  try {
    const info = await localInvoke("remote_call", { id: c.id, cmd: "system_info", args: {} });
    const ms = Date.now() - t0;
    const srv = await localInvoke("remote_call", { id: c.id, cmd: "server_info", args: {} }).catch(() => null);
    return { ok: true, info, srv, ms };
  } catch (e) {
    return { ok: false, err: String(e) };
  }
}

function drawFleet() {
  const local = S.info || {};
  const card = (c) => {
    const st = FLEET.status[c.id] || {};
    if (st.loading) return `<div class="card mcard"><div class="spread"><b>${esc(c.name)}</b><span class="spin sm"></span></div><div class="muted mono" style="font-size:12px">${esc(c.url)}</div></div>`;
    if (!st.ok) return `<div class="card mcard"><div class="spread"><b>${esc(c.name)}</b><span class="pill high">offline</span></div><div class="muted mono" style="font-size:12px">${esc(c.url)}</div>
      <div class="cell-sub danger-t" style="margin:8px 0">${esc(st.err || "")}</div><div class="row"><button class="btn small" data-edit="${esc(c.id)}">Edit</button><button class="btn small danger" data-rm="${esc(c.id)}">Remove</button></div></div>`;
    const i = st.info, s = st.srv || {};
    const guests = arr(s.proxmox?.guests);
    const up = guests.filter((g) => g.status === "running").length;
    const zfsBad = arr(s.zfs).filter((z) => z.health !== "ONLINE").length;
    const smartBad = arr(s.smart).filter((d) => d.passed === false).length;
    const disk = arr(i.disks).sort((a, b) => b.total - a.total)[0];
    const issues = [arr(s.failed).length && `${arr(s.failed).length} failed service(s)`, zfsBad && `${zfsBad} ZFS pool(s) degraded`, smartBad && `${smartBad} disk(s) failing SMART`, disk && disk.free / disk.total < 0.1 && `${esc(disk.mount)} almost full`].filter(Boolean);
    return `<div class="card mcard"><div class="spread"><b>${esc(c.name)}</b><span class="pill ${issues.length ? "medium" : "low"}">${issues.length ? "needs attention" : "healthy"}</span></div>
      <div class="muted" style="font-size:12.5px;margin:2px 0 10px">${esc(i.hostname)} · ${esc({ proxmox: "Proxmox VE", unraid: "Unraid", linux: "Linux", windows: "Windows" }[s.platform] || i.os)} · ${st.ms} ms</div>
      <div class="grid g3" style="gap:8px">
        <div><div class="label">Load</div><div class="v">${esc((s.load || "").split(" ")[0] || "-")}</div></div>
        <div><div class="label">RAM</div><div class="v">${i.memory_total ? Math.round((i.memory_used / i.memory_total) * 100) + "%" : "-"}</div></div>
        <div><div class="label">${guests.length ? "Guests up" : "Disk free"}</div><div class="v">${guests.length ? `${up}/${guests.length}` : disk ? bytes(disk.free) : "-"}</div></div>
      </div>
      ${issues.length ? `<div class="cell-sub warn" style="margin-top:8px">${issues.join(" · ")}</div>` : ""}
      <div class="row" style="margin-top:12px"><button class="btn small primary" data-open="${esc(c.id)}">Manage</button><button class="btn small" data-edit="${esc(c.id)}">Edit</button><button class="btn small danger" data-rm="${esc(c.id)}">Remove</button></div></div>`;
  };
  $("#page").innerHTML = `
    <p class="muted" style="margin-top:0">Every machine running MWM, in one place. Install it on a server with <span class="mono">curl -fsSL https://github.com/BlizzHacker/mwm/releases/latest/download/install.sh | sh -s -- --web</span>, then add it here with the address and token it prints.</p>
    <div class="grid g3">
      <div class="card mcard" style="border-color:var(--accent)"><div class="spread"><b>This PC</b><span class="pill low">local</span></div>
        <div class="muted" style="font-size:12.5px;margin:2px 0 10px">${esc(local.hostname || "")} · ${esc(local.os || "")}</div>
        <div class="grid g3" style="gap:8px"><div><div class="label">Cores</div><div class="v">${esc(local.cores || "-")}</div></div><div><div class="label">RAM</div><div class="v">${local.memory_total ? Math.round((local.memory_used / local.memory_total) * 100) + "%" : "-"}</div></div><div><div class="label">Disk free</div><div class="v">${arr(local.disks)[0] ? bytes(arr(local.disks)[0].free) : "-"}</div></div></div>
        <div class="row" style="margin-top:12px"><button class="btn small primary" data-open="">Manage</button></div></div>
      ${FLEET.conns.map(card).join("")}
      <div class="card mcard" style="display:grid;place-items:center;min-height:170px;cursor:pointer;border-style:dashed" id="fl-add2"><div style="text-align:center"><div style="font-size:30px">＋</div><b>Add a machine</b><div class="muted">Proxmox, Unraid, Linux or another PC</div></div></div>
    </div>`;
  $("#fl-add2").onclick = () => editConn();
  $$("[data-open]").forEach((b) => (b.onclick = () => switchTo(b.dataset.open)));
  $$("[data-edit]").forEach((b) => (b.onclick = () => editConn(FLEET.conns.find((c) => c.id === b.dataset.edit))));
  $$("[data-rm]").forEach((b) => (b.onclick = async () => {
    const c = FLEET.conns.find((x) => x.id === b.dataset.rm);
    if (!(await choose(`Remove ${esc(c.name)}?`, `<p class="muted">Only the saved connection is removed - nothing on the server changes.</p>`, [["go", "Remove", "danger"]]))) return;
    await guard(() => localInvoke("conn_remove", { id: c.id }));
    delete FLEET.status[c.id];
    if (TARGET === c.id) await switchTo("");
    VIEWS.fleet();
    renderSwitcher();
  }));
}

function editConn(c) {
  modal(`<h2>${c ? `Edit ${esc(c.name)}` : "Add a machine"}</h2>
    <p class="muted">On the server run <span class="mono">mwm serve --bind 0.0.0.0:7777</span> (the installer's <span class="mono">--web</span> option does this for you). Paste the address - or the whole sign-in link it prints - and the token.</p>
    <label class="muted">Name<input class="search" id="c-name" style="width:100%;margin:4px 0 10px" value="${esc(c?.name || "")}" placeholder="pve1, unraid, office-pc"></label>
    <label class="muted">Address<input class="search mono" id="c-url" style="width:100%;margin:4px 0 10px" value="${esc(c?.url || "")}" placeholder="http://192.168.0.6:7777  or the full ?token= link"></label>
    <label class="muted">Access token ${c ? "(leave blank to keep the saved one)" : ""}<input class="search mono" id="c-tok" type="password" style="width:100%;margin-top:4px" placeholder="mwm serve --show-token"></label>
    <div id="c-msg" style="margin-top:10px"></div>
    <div class="row" style="justify-content:flex-end;margin-top:14px"><button class="btn" data-close>Cancel</button><button class="btn primary" id="c-save">Test &amp; save</button></div>`);
  $("#c-name").focus();
  $("#c-save").onclick = async () => {
    const msg = $("#c-msg");
    msg.innerHTML = '<span class="spin sm"></span> Connecting...';
    let saved;
    try {
      saved = await localInvoke("conn_save", { id: c?.id || null, name: $("#c-name").value, url: $("#c-url").value, token: $("#c-tok").value });
      const info = await localInvoke("remote_call", { id: saved.id, cmd: "system_info", args: {} });
      closeModal();
      toast(`Connected to ${info.hostname} (${info.os}).`);
      delete FLEET.status[saved.id];
      await renderSwitcher();
      if (current === "fleet") VIEWS.fleet();
    } catch (e) {
      msg.innerHTML = `<span class="danger-t">${esc(String(e))}</span>${saved ? `<div class="cell-sub">Saved anyway - fix it with Edit.</div>` : ""}`;
      await loadConns();
    }
  };
}
