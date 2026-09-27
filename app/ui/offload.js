/* MWM - Move Weight: ship heavy folders to your own server over SSH (only what
   exists nowhere else), delete them here, and let the guard keep the drive
   from growing back. Engine: mwm-core/src/offload.rs. */
"use strict";

const OFF = { st: null, sel: new Set(), server: null, filter: "all" };

async function offLoad() {
  OFF.st = await guard(() => invoke("offload_status"));
}

VIEWS.offload = async function () {
  $("#page").innerHTML = loading("Loading...");
  await offLoad();
  if (current !== "offload") return;
  drawOffload();
  if (OFF.st?.policy?.server && OFF.server === null) {
    OFF.server = { message: "checking..." };
    OFF.server = await invoke("offload_server_check").catch((e) => ({ ok: false, message: String(e) }));
    if (current === "offload") drawOffload();
  }
};

const offVerdict = (v) =>
  v === "safe" ? '<span class="pill low">ready</span>' : v === "keep" ? '<span class="pill">kept</span>' : '<span class="pill medium">review</span>';

function offDetail(i) {
  const bits = [];
  if (i.kind === "rebuildable") bits.push("rebuildable - deleted, nothing travels");
  else if (i.kind === "bare") {
    bits.push("bare git repo");
    if (i.unpushed) bits.push(`${i.unpushed} commit${i.unpushed > 1 ? "s" : ""} no remote has`);
    if (!i.remotes?.length) bits.push("no remote");
  } else if (i.kind === "repo" || i.kind === "worktree") {
    bits.push(i.kind === "worktree" ? "git worktree" : "git repo");
    if (i.unpushed) bits.push(`${i.unpushed} unpushed commit${i.unpushed > 1 ? "s" : ""}`);
    if (i.stashes) bits.push(`${i.stashes} stash${i.stashes > 1 ? "es" : ""}`);
    if (i.dirty) bits.push(`${i.dirty} uncommitted`);
    if (!i.remotes?.length) bits.push("no remote");
    if (i.dropBytes) bits.push(`${bytes(i.dropBytes)} rebuildable, dropped`);
  } else bits.push("folder, shipped whole");
  if (i.note) bits.push(i.note);
  return bits.join(" · ");
}

function drawOffload() {
  const st = OFF.st || {};
  const p = st.policy || {};
  const scan = st.scan?.items || [];
  const drive = st.drive;
  const configured = !!p.server && (p.roots?.length || p.items?.length);
  const running = JOBS.list.some((j) => j.state === "running" && j.page === "offload");
  const shown = scan.filter((i) => OFF.filter === "all" || i.verdict === OFF.filter);
  const sel = scan.filter((i) => OFF.sel.has(i.path));
  const selBytes = sel.reduce((a, i) => a + i.bytes, 0);
  const selShip = sel.reduce((a, i) => a + i.shipBytes, 0);
  const movable = scan.filter((i) => i.verdict !== "keep").reduce((a, i) => a + i.bytes, 0);
  const moved = (st.ledger || []).reduce((a, m) => a + m.bytes, 0);

  $("#top-actions").innerHTML = `<button class="btn" id="of-set">${icon("wrench")}Settings</button>
    <button class="btn" id="of-scan" ${configured && !running ? "" : "disabled"}>${icon("search")}${st.scan ? "Scan again" : "Scan"}</button>`;
  $("#page").innerHTML = `
    <div class="card hero">
      ${gauge(drive ? 1 - drive.free / drive.total : 0, drive ? `${Math.round((1 - drive.free / drive.total) * 100)}%` : "--", drive ? `${esc(drive.mount)} used` : "drive")}
      <div style="flex:1">
        <div class="label">Move Weight - your server holds it, this drive doesn't</div>
        <h2>${
          !configured ? "Point MWM at your server and the folders to watch."
          : st.scan ? `<span class="accent">${bytes(movable)}</span> can leave this drive.`
          : "Scan to see what can move."
        }</h2>
        <div class="muted">${drive ? `${bytes(drive.free)} free of ${bytes(drive.total)} on ${esc(drive.mount)}` : ""}${
          drive && drive.free / drive.total < 0.05 ? ' &nbsp;<span class="danger-t">- critically low</span>' : ""}
          ${moved ? ` &nbsp;·&nbsp; ${bytes(moved)} moved so far` : ""}</div>
        <div class="muted" style="margin-top:6px">${p.server ? `Server <b>${esc(p.server)}</b> : <span class="mono">${esc(p.remoteRoot)}</span> - ${
          OFF.server ? (OFF.server.ok ? `<span class="accent">${esc(OFF.server.message)}</span>` : `<span class="danger-t">${esc(OFF.server.message || "")}</span>`) : ""}` : ""}</div>
      </div>
      <div class="stack" style="text-align:right;min-width:210px">
        <div class="row" style="justify-content:flex-end;gap:10px"><span class="muted">Guard</span><button class="toggle ${p.guard ? "on" : ""}" id="of-guard" ${configured ? "" : "disabled"} title="Daily: move folders idle ${p.idleDays || 3}+ days, clean safe caches"></button></div>
        <div class="faint" style="font-size:12px">${p.guard ? (st.guardInstalled ? `On - daily at 4:15 and at sign-in; moves folders idle ${p.idleDays}+ days` : "On, but the scheduled task is missing - toggle it again") : "Off - nothing moves on its own"}</div>
        <button class="btn small" id="of-now" ${configured && !running ? "" : "disabled"}>Run the guard now</button>
      </div>
    </div>
    <div class="note" style="margin:14px 0">Git repos keep only what no remote has: unpushed commits and stashes (a bundle), uncommitted changes, untracked files and ignored files that cannot be rebuilt. <b>node_modules, target, dist</b> and friends are dropped; everything else is re-cloned from GitHub when you need it. Other folders travel whole. Every shipment is checksummed on the server before anything here is deleted.</div>
    ${!st.scan ? `<div class="empty">${icon("weight")}<h3>${configured ? "Ready when you are" : "Set it up first"}</h3>${configured ? "Scan measures every watched folder and works out what would have to travel." : "Settings: your server (user@host with SSH keys), where shipments go, and the folders to watch."}</div>` : `
    <div class="row" style="margin-bottom:10px;gap:6px">${["all", "safe", "review", "keep"].map((f) => `<button class="btn small ${OFF.filter === f ? "primary" : ""}" data-off-f="${f}">${f === "safe" ? "Ready" : f[0].toUpperCase() + f.slice(1)} (${f === "all" ? scan.length : scan.filter((i) => i.verdict === f).length})</button>`).join("")}
      <span class="faint" style="margin-left:auto">Scanned ${new Date(st.scan.when * 1000).toLocaleString()}</span></div>
    <div class="card" style="padding:0"><div class="scroll" style="max-height:52vh"><table class="table"><thead><tr>
      <th style="width:34px"><input type="checkbox" class="cb" id="of-all"></th><th>Folder</th><th class="num">Idle</th><th class="num">Here</th><th class="num">Travels</th><th></th></tr></thead><tbody>
      ${shown.map((i) => `<tr>
        <td><input type="checkbox" class="cb" data-off="${esc(i.path)}" ${OFF.sel.has(i.path) ? "checked" : ""} ${i.verdict === "keep" ? "disabled" : ""}></td>
        <td><div class="cell-main">${esc(i.name)}</div><div class="cell-sub">${esc(offDetail(i))}</div></td>
        <td class="num muted">${i.idleDays}d</td><td class="num"><b>${bytes(i.bytes)}</b></td><td class="num">${bytes(i.shipBytes)}</td><td>${offVerdict(i.verdict)}</td></tr>`).join("")}
    </tbody></table></div></div>`}
    ${(st.ledger || []).length ? `<div class="card" style="margin-top:14px"><div class="spread"><div class="label">Moved to the server</div><span class="muted">${(st.ledger || []).length} folders · ${bytes(moved)}</span></div><div class="sep"></div>
      <div class="scroll" style="max-height:30vh"><table class="table"><tbody>${st.ledger.map((m, k) => `<tr><td><div class="cell-main">${esc(m.name)}</div><div class="cell-sub mono">${esc(m.server)}:${esc(m.dest)}</div></td>
        <td class="num">${bytes(m.bytes)}</td><td class="num faint">${new Date(m.when * 1000).toLocaleDateString()}</td><td><button class="btn small" data-off-restore="${k}">Restore steps</button></td></tr>`).join("")}</tbody></table></div></div>` : ""}
    ${st.guardLog ? `<details class="card" style="margin-top:14px"><summary class="label">Guard log</summary><pre class="mono" style="white-space:pre-wrap;max-height:30vh;overflow:auto">${esc(st.guardLog)}</pre></details>` : ""}
    ${st.scan ? `<div class="footer-bar"><span class="muted">${sel.length} selected · ${bytes(selShip)} travels</span><b style="font-size:16px">${bytes(selBytes)} freed</b>
      <button class="btn primary" id="of-go" ${sel.length && !running ? "" : "disabled"}>${icon("weight")}Move to the server</button></div>` : ""}`;

  $("#of-set").onclick = offSettings;
  $("#of-scan") && ($("#of-scan").onclick = () => guard(async () => {
    const id = await invoke("offload_scan");
    watchJob(id, async () => { await offLoad(); if (current === "offload") drawOffload(); });
    drawOffload();
  }));
  $("#of-guard") && ($("#of-guard").onclick = () => guard(async () => {
    const pol = { ...p, guard: !p.guard };
    await invoke("offload_policy_set", { policy: pol });
    await offLoad();
    toast(pol.guard ? "Guard on: idle folders move to the server every day." : "Guard off.");
    drawOffload();
  }));
  $("#of-now") && ($("#of-now").onclick = () => guard(async () => {
    await invoke("offload_guard_now");
    toast("Guard running - progress is in the sidebar.");
    setTimeout(() => { pollJobs(); }, 800);
  }));
  $$("[data-off-f]").forEach((b) => (b.onclick = () => { OFF.filter = b.dataset.offF; drawOffload(); }));
  $$("[data-off]").forEach((cb) => (cb.onchange = () => { cb.checked ? OFF.sel.add(cb.dataset.off) : OFF.sel.delete(cb.dataset.off); drawOffload(); }));
  $("#of-all") && ($("#of-all").onchange = (e) => { shown.filter((i) => i.verdict !== "keep").forEach((i) => (e.target.checked ? OFF.sel.add(i.path) : OFF.sel.delete(i.path))); drawOffload(); });
  $$("[data-off-restore]").forEach((b) => (b.onclick = () => {
    const m = st.ledger[+b.dataset.offRestore];
    modal(`<h2>${esc(m.name)}</h2><p class="muted">On <b>${esc(m.server)}</b> in <span class="mono">${esc(m.dest)}</span> (RESTORE.md is there too).</p>
      <pre class="mono" style="white-space:pre-wrap;max-height:50vh;overflow:auto">${esc(m.restore)}</pre>
      <div class="row" style="justify-content:flex-end;margin-top:14px"><button class="btn" id="of-cp">Copy</button><button class="btn primary" data-close>Close</button></div>`);
    $("#of-cp").onclick = () => copyText(m.restore);
  }));
  $("#of-go") && ($("#of-go").onclick = async () => {
    const ch = await choose("Move to the server?",
      `<p>${sel.length} folder(s), <b>${bytes(selBytes)}</b> leaves this drive. About <b>${bytes(selShip)}</b> travels to <b>${esc(p.server)}</b>; the rest is already on a remote or rebuildable.</p><p class="muted">Each folder is deleted here only after its shipment is verified on the server.</p>`,
      [["go", "Move them", "primary"]]);
    if (ch !== "go") return;
    guard(async () => {
      const id = await invoke("offload_run", { paths: sel.map((i) => i.path) });
      OFF.sel.clear();
      watchJob(id, async () => { await offLoad(); if (current === "offload") drawOffload(); });
      drawOffload();
    });
  });
}

async function offSettings() {
  const p = OFF.st?.policy || {};
  modal(`<h2>Move Weight settings</h2>
    <label class="muted" style="font-size:12px">Server (user@host, signs in with your SSH key)</label>
    <input class="search" id="os-server" style="width:100%;margin:4px 0 10px" value="${esc(p.server || "")}" placeholder="root@192.168.0.6">
    <label class="muted" style="font-size:12px">Folder on the server for shipments</label>
    <input class="search" id="os-root" style="width:100%;margin:4px 0 10px" value="${esc(p.remoteRoot || "mwm-offload")}">
    <label class="muted" style="font-size:12px">Watched folders: every sub-folder is a candidate (one per line)</label>
    <textarea class="search" id="os-roots" style="width:100%;height:70px;margin:4px 0 10px">${esc((p.roots || []).join("\n"))}</textarea>
    <label class="muted" style="font-size:12px">Single folders that are candidates themselves (one per line)</label>
    <textarea class="search" id="os-items" style="width:100%;height:56px;margin:4px 0 10px">${esc((p.items || []).join("\n"))}</textarea>
    <label class="muted" style="font-size:12px">Never move (names or full paths, one per line)</label>
    <textarea class="search" id="os-keep" style="width:100%;height:56px;margin:4px 0 10px">${esc((p.keep || []).join("\n"))}</textarea>
    <div class="row" style="gap:14px;flex-wrap:wrap">
      <label class="muted">Guard moves folders idle <input class="search" id="os-idle" type="number" min="0" style="width:64px" value="${p.idleDays ?? 3}"> days</label>
      <label class="muted">Leave shipments over <input class="search" id="os-max" type="number" min="1" style="width:64px" value="${p.maxShipGb ?? 8}"> GB for review</label>
      <label class="row muted" style="gap:6px"><input type="checkbox" class="cb" id="os-caches" ${p.cleanCaches !== false ? "checked" : ""}>Guard cleans safe caches</label>
    </div>
    <div class="row" style="justify-content:flex-end;margin-top:18px"><button class="btn" data-close>Cancel</button><button class="btn primary" id="os-save">Save</button></div>`);
  $("#os-save").onclick = () => guard(async () => {
    const lines = (id) => $(id).value.split(/\r?\n/).map((s) => s.trim()).filter(Boolean);
    const pol = {
      ...p,
      server: $("#os-server").value.trim(),
      remoteRoot: $("#os-root").value.trim() || "mwm-offload",
      roots: lines("#os-roots"),
      items: lines("#os-items"),
      keep: lines("#os-keep"),
      idleDays: Math.max(0, +$("#os-idle").value || 0),
      maxShipGb: Math.max(1, +$("#os-max").value || 8),
      cleanCaches: $("#os-caches").checked,
    };
    await invoke("offload_policy_set", { policy: pol });
    closeModal();
    OFF.server = null;
    VIEWS.offload();
  });
}
