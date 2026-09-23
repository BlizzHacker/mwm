/* MDW - Move Digital Weight. Plain JS UI over the Tauri commands in main.rs.
   Outside Tauri (a normal browser) it runs against demo data in demo.js. */
"use strict";

const TAURI = window.__TAURI__;
const DEMO = !TAURI;
const invoke = (cmd, args = {}) => (DEMO ? window.MDW_DEMO(cmd, args) : TAURI.core.invoke(cmd, args));

// ------------------------------------------------------------ helpers ----
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const esc = (v) => String(v ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
function bytes(n) {
  if (!n) return "0 B";
  const u = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
  return `${n.toFixed(i && n < 100 ? 1 : 0)} ${u[i]}`;
}
const num = (n) => Number(n || 0).toLocaleString();
function toast(msg, err = false) {
  const t = document.createElement("div");
  t.className = "toast" + (err ? " err" : "");
  t.textContent = msg;
  $("#toasts").appendChild(t);
  setTimeout(() => t.remove(), err ? 7000 : 4200);
}
const loading = (msg) => `<div class="loading"><span class="spin lg"></span><div>${esc(msg)}</div></div>`;
function modal(html) { $("#modal-card").innerHTML = html; $("#modal").classList.remove("hidden"); }
function closeModal() { $("#modal").classList.add("hidden"); document.dispatchEvent(new CustomEvent("mdw-modal-closed")); }
document.addEventListener("keydown", (e) => { if (e.key === "Escape" && modalOpen()) closeModal(); });
const modalOpen = () => !$("#modal").classList.contains("hidden");
/** Small promise-based prompt: resolves to the entered string or null. */
function ask(title, label, value = "", okText = "OK", note = "") {
  return new Promise((resolve) => {
    modal(`<h2>${esc(title)}</h2>${note ? `<p class="muted">${note}</p>` : ""}<label class="muted" style="font-size:12px">${esc(label)}</label>
      <input class="search" id="ask-v" style="width:100%;margin-top:6px" value="${esc(value)}">
      <div class="row" style="justify-content:flex-end;margin-top:18px"><button class="btn" id="ask-no">Cancel</button><button class="btn primary" id="ask-ok">${esc(okText)}</button></div>`);
    const inp = $("#ask-v");
    inp.focus();
    const dot = value.lastIndexOf(".");
    inp.setSelectionRange(0, dot > 0 ? dot : value.length);
    const done = (v) => { closeModal(); resolve(v); };
    $("#ask-ok").onclick = () => done(inp.value);
    $("#ask-no").onclick = () => done(null);
    inp.onkeydown = (e) => { if (e.key === "Enter") done(inp.value); if (e.key === "Escape") done(null); };
  });
}
/** Promise-based choice dialog: resolves to the chosen key or null. */
function choose(title, html, buttons) {
  return new Promise((resolve) => {
    modal(`<h2>${title}</h2>${html}<div class="row" style="justify-content:flex-end;margin-top:18px;flex-wrap:wrap"><button class="btn" data-ch="">Cancel</button>${buttons.map(([k, t, cls]) => `<button class="btn ${cls || ""}" data-ch="${k}">${esc(t)}</button>`).join("")}</div>`);
    $$("[data-ch]").forEach((b) => (b.onclick = () => { closeModal(); resolve(b.dataset.ch || null); }));
    const primary = $$("[data-ch]").pop();
    primary && primary.focus();
  });
}
async function guard(fn) {
  try { return await fn(); } catch (e) { toast(String(e?.message || e), true); console.error(e); }
}

const I = {
  health: '<path d="M12 21s-7-4.4-9.3-9A5.2 5.2 0 0 1 12 6.3 5.2 5.2 0 0 1 21.3 12C19 16.6 12 21 12 21z"/><path d="M3.5 12h4l2-3 3 6 2-3h6"/>',
  clean: '<path d="M4 20h16"/><path d="M6 20l1.5-8h9L18 20"/><path d="M10 12V4h4v8"/>',
  apps: '<rect x="3" y="4" width="18" height="14" rx="2"/><path d="M3 8h18"/><path d="M9 13l2 2 4-4"/>',
  uninstall: '<path d="M4 7h16"/><path d="M9 7V4h6v3"/><path d="M6 7l1 13h10l1-13"/><path d="M10 11v6M14 11v6"/>',
  startup: '<path d="M12 3v9"/><path d="M6.3 6.3a8 8 0 1 0 11.4 0"/>',
  updates: '<path d="M21 12a9 9 0 1 1-3-6.7"/><path d="M21 4v5h-5"/>',
  dupes: '<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M4 16V6a2 2 0 0 1 2-2h10"/>',
  disk: '<circle cx="12" cy="12" r="9"/><path d="M12 3v9l6.5 6"/>',
  perf: '<path d="M4 18l5-6 4 3 7-9"/><path d="M15 6h5v5"/>',
  drivers: '<rect x="5" y="5" width="14" height="14" rx="2"/><path d="M9 1v4M15 1v4M9 19v4M15 19v4M1 9h4M1 15h4M19 9h4M19 15h4"/><rect x="9" y="9" width="6" height="6"/>',
  tools: '<path d="M14.7 6.3a4 4 0 0 0-5.4 5.4L3 18l3 3 6.3-6.3a4 4 0 0 0 5.4-5.4l-2.5 2.5-2.5-.5-.5-2.5z"/>',
  shred: '<rect x="5" y="3" width="14" height="9" rx="1"/><path d="M3 12h18M7 12v9M10 12v7M14 12v9M17 12v6"/>',
  about: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7.5v.5"/>',
  shield: '<path d="M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z"/>',
  folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
  play: '<path d="M7 4l13 8-13 8z"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="M20 20l-3.5-3.5"/>',
  report: '<rect x="5" y="3" width="14" height="18" rx="2"/><path d="M9 8h6M9 12h6M9 16h4"/>',
  wrench: '<path d="M14.7 6.3a4 4 0 0 0-5.4 5.4L3 18l3 3 6.3-6.3a4 4 0 0 0 5.4-5.4l-2.5 2.5-2.5-.5-.5-2.5z"/>',
  net: '<path d="M2 9a15 15 0 0 1 20 0"/><path d="M5.5 12.5a10 10 0 0 1 13 0"/><path d="M9 16a5 5 0 0 1 6 0"/><circle cx="12" cy="19.5" r="1"/>',
  alert: '<path d="M12 3l10 18H2z"/><path d="M12 10v5M12 18v.5"/>',
  weight: '<path d="M6 9h12l2 11H4z"/><circle cx="12" cy="6" r="3"/>',
};
const icon = (k) => `<svg viewBox="0 0 24 24">${I[k] || ""}</svg>`;

// -------------------------------------------------------------- state ----
const S = {
  info: null,
  scan: null, // cleaner scan items
  checked: new Set(),
  apps: null,
  startup: null,
  updates: null,
  lastClean: null,
};

const PAGES = [
  { sec: "Overview" },
  { id: "health", label: "Health Check", icon: "health" },
  { sec: "Clean" },
  { id: "cleaner", label: "Custom Clean", icon: "clean" },
  { id: "dupes", label: "Duplicate Finder", icon: "dupes" },
  { id: "disk", label: "Disk Analyzer", icon: "disk" },
  { sec: "Files" },
  { id: "files", label: "Commander", icon: "folder" },
  { sec: "Programs" },
  { id: "uninstaller", label: "Uninstaller", icon: "uninstall" },
  { id: "startup", label: "Startup Manager", icon: "startup" },
  { id: "updates", label: "Software Updater", icon: "updates" },
  { id: "perf", label: "Performance", icon: "perf" },
  { id: "drivers", label: "Drivers", icon: "drivers" },
  { sec: "Tech Toolkit" },
  { id: "report", label: "System Report", icon: "report" },
  { id: "repair", label: "Repair", icon: "wrench" },
  { id: "security", label: "Security", icon: "shield" },
  { id: "network", label: "Network", icon: "net" },
  { id: "events", label: "Crashes & Events", icon: "alert" },
  { sec: "Privacy & tools" },
  { id: "privacy", label: "Shredder & Wipe", icon: "shred" },
  { id: "tools", label: "System Tools", icon: "tools" },
  { id: "about", label: "About MDW", icon: "about" },
];
let current = "health";

// Background activity per page (long jobs keep running across tab changes).
// ACT = UI-side jobs; JOBS = engine jobs (copies, repairs) polled from Rust.
const ACT = {};
const JOBS = { list: [], watchers: {}, timer: null };
function setAct(page, text) {
  if (text) ACT[page] = text; else delete ACT[page];
  renderNav();
  renderActivity();
}
const pageBusy = (id) => !!ACT[id] || JOBS.list.some((j) => j.state === "running" && j.page === id);
function jobPct(j) {
  if (j.total_bytes) return Math.min(100, (j.done_bytes / j.total_bytes) * 100);
  if (j.total_items) return Math.min(100, (j.done_items / j.total_items) * 100);
  return null;
}
function renderActivity() {
  const box = document.getElementById("activity");
  if (!box) return;
  const ui = Object.entries(ACT).map(([p, t]) => `<div class="act" data-go="${p}"><span class="spin sm"></span><span class="act-t">${esc(t)}</span></div>`);
  const eng = JOBS.list.filter((j) => j.state === "running").map((j) => {
    const pct = jobPct(j);
    return `<div class="act" data-go="${esc(j.page)}"><span class="spin sm"></span><span class="act-t">${esc(j.title)}${pct !== null ? ` <b>${pct.toFixed(0)}%</b>` : ""}</span><button class="act-x" data-cancel="${j.id}" title="Cancel">✕</button>${pct !== null ? `<i class="act-bar" style="width:${pct}%"></i>` : ""}</div>`;
  });
  box.innerHTML = ui.concat(eng).join("");
}
/** Track an engine job; `onDone(job)` runs when it finishes, whatever page is open. */
function watchJob(id, onDone, quiet = false) {
  JOBS.watchers[id] = { onDone, quiet };
  pollJobs();
}
function pollJobs() { if (!JOBS.timer) JOBS.timer = setTimeout(jobTick, 60); }
async function jobTick() {
  JOBS.timer = null;
  try { JOBS.list = await invoke("jobs_list"); } catch { JOBS.list = []; }
  for (const j of JOBS.list) {
    const w = JOBS.watchers[j.id];
    if (w && j.state !== "running") {
      delete JOBS.watchers[j.id];
      if (!w.quiet) toast(`${j.title} - ${j.message || j.state}`, j.state === "failed");
      try { w.onDone && w.onDone(j); } catch (e) { console.error(e); }
    }
  }
  renderActivity();
  renderNav();
  document.dispatchEvent(new CustomEvent("mdw-jobs"));
  if (JOBS.list.some((j) => j.state === "running") || Object.keys(JOBS.watchers).length) JOBS.timer = setTimeout(jobTick, 600);
}
document.addEventListener("click", (e) => {
  const c = e.target.closest("[data-cancel]");
  if (c) { e.stopPropagation(); invoke("job_cancel", { id: +c.dataset.cancel }); }
}, true);

function renderNav() {
  $("#nav").innerHTML = PAGES.map((p) =>
    p.sec
      ? `<div class="nav-sec">${esc(p.sec)}</div>`
      : `<button class="nav-item ${p.id === current ? "active" : ""}" data-go="${p.id}">${icon(p.icon)}<span>${esc(p.label)}</span>${
          pageBusy(p.id) ? '<span class="spin sm nav-spin"></span>' : p.id === "updates" && S.updates && updPending().length ? `<span class="badge">${updPending().length}</span>` : ""
        }</button>`
  ).join("");
}

function go(id) {
  if (current !== id) document.dispatchEvent(new CustomEvent("mdw-leave", { detail: current }));
  current = id;
  renderNav();
  const p = PAGES.find((x) => x.id === id);
  $("#title").textContent = p.label;
  $("#top-actions").innerHTML = "";
  $("#page").scrollTop = 0;
  VIEWS[id]();
}

document.addEventListener("click", (e) => {
  const g = e.target.closest("[data-go]");
  if (g) go(g.dataset.go);
  if (e.target.id === "modal" || e.target.closest("[data-close]")) closeModal();
});

function adminNote(needed) {
  if (!needed || S.info?.elevated) return "";
  return `<div class="note warn-n" style="margin-bottom:14px">Some items need administrator rights. <a href="#" data-admin>Restart MDW as administrator</a> to include them.</div>`;
}
document.addEventListener("click", (e) => {
  if (e.target.closest("[data-admin]")) {
    e.preventDefault();
    guard(() => invoke("relaunch_admin"));
  }
});

// ======================================================= HEALTH CHECK ====
function gauge(pct, big, sub) {
  const r = 62, c = 2 * Math.PI * r;
  const off = c * (1 - Math.max(0, Math.min(1, pct)));
  return `<svg class="gauge" viewBox="0 0 150 150"><circle class="track" cx="75" cy="75" r="${r}"/>
    <circle class="val" cx="75" cy="75" r="${r}" stroke-dasharray="${c}" stroke-dashoffset="${off}" transform="rotate(-90 75 75)"/>
    <text x="75" y="76">${esc(big)}</text><text class="sub" x="75" y="94">${esc(sub)}</text></svg>`;
}

const VIEWS = {};
VIEWS.health = async function () {
  const sys = S.info;
  const main = sys?.disks?.find((d) => /^[A-Z]:\\?$/i.test(d.mount) && d.mount.toUpperCase().startsWith("C")) || sys?.disks?.[0];
  const junk = S.scan ? S.scan.filter((i) => i.default_on && (!i.admin || sys?.elevated)).reduce((a, i) => a + i.bytes, 0) : null;
  const enabledStartup = S.startup ? S.startup.filter((i) => i.enabled && (i.kind === "run" || i.kind === "folder")).length : null;
  const upd = S.updates ? S.updates.length : null;
  const scanned = S.scan && S.startup;
  const usedPct = main ? 1 - main.free / main.total : 0;
  const cleaned = S.lastClean;

  $("#top-actions").innerHTML = `<button class="btn" id="hc-scan">${icon("search")}${scanned ? "Scan again" : "Scan now"}</button>`;
  $("#page").innerHTML = `
    <div class="card hero">
      ${gauge(scanned ? usedPct : 0, main ? `${Math.round(usedPct * 100)}%` : "--", main ? `${esc(main.mount)} used` : "disk")}
      <div style="flex:1">
        <div class="label">Digital weight on this machine</div>
        <h2>${
          cleaned ? `You just moved <span class="accent">${bytes(cleaned)}</span> of digital weight.`
          : junk === null ? "Let's see what's weighing your PC down."
          : junk > 0 ? `<span class="accent">${bytes(junk)}</span> of junk is ready to go.`
          : "Lean and clean. Nothing to move right now."
        }</h2>
        <div class="muted">${main ? `${bytes(main.free)} free of ${bytes(main.total)} on ${esc(main.mount)}` : ""}${
          main && main.free / main.total < 0.05 ? ` &nbsp;<span class="danger-t">- critically low</span>` : ""
        }</div>
        <div class="steps">
          <div class="step ${scanned ? "done" : ""}"><span class="n">1</span>Scan</div>
          <div class="step ${scanned ? "done" : ""}"><span class="n">2</span>Review</div>
          <div class="step ${cleaned ? "done" : ""}"><span class="n">3</span>Move the weight</div>
        </div>
      </div>
      <div class="stack" style="text-align:right">
        <button class="btn primary" id="hc-clean" ${junk ? "" : "disabled"}>${icon("weight")}Move the weight</button>
        <div class="faint" style="font-size:12px">Cleans only the safe, default items.</div>
      </div>
    </div>
    <div class="grid g4" style="margin-top:14px">
      <div class="card stat" data-go="cleaner"><div class="label">Junk files</div><div class="v">${junk === null ? '<span class="spin"></span>' : bytes(junk)}</div><div class="muted">Temp, caches, recycle bin</div></div>
      <div class="card stat" data-go="startup"><div class="label">Startup apps</div><div class="v">${enabledStartup === null ? '<span class="spin"></span>' : enabledStartup}</div><div class="muted">Launch when you sign in</div></div>
      <div class="card stat" data-go="updates"><div class="label">Outdated apps</div><div class="v ${upd ? "warn" : ""}">${upd === null ? '<span class="spin"></span>' : upd}</div><div class="muted">Security & bug fixes waiting</div></div>
      <div class="card stat" data-go="uninstaller"><div class="label">Installed programs</div><div class="v">${S.apps ? S.apps.length : '<span class="spin"></span>'}</div><div class="muted">Review what you never use</div></div>
    </div>
    <div class="grid g2" style="margin-top:14px">
      <div class="card"><div class="label" style="margin-bottom:10px">Drives</div>${(sys?.disks || [])
        .map((d) => {
          const p = 1 - d.free / d.total;
          return `<div style="margin-bottom:12px"><div class="spread"><b>${esc(d.mount)}</b><span class="muted">${bytes(d.free)} free / ${bytes(d.total)}</span></div><div class="bar ${p > 0.93 ? "full" : ""}"><i style="width:${(p * 100).toFixed(1)}%"></i></div></div>`;
        })
        .join("")}</div>
      <div class="card"><div class="label" style="margin-bottom:10px">Biggest junk right now</div>${
        S.scan
          ? S.scan.filter((i) => i.bytes > 0).sort((a, b) => b.bytes - a.bytes).slice(0, 6)
              .map((i) => `<div class="spread" style="padding:5px 0"><span>${esc(i.category)} - ${esc(i.name)}${i.default_on ? "" : ' <span class="faint">(opt-in)</span>'}</span><b>${bytes(i.bytes)}</b></div>`).join("") || '<div class="muted">Nothing found.</div>'
          : loading("Scanning...")
      }</div>
    </div>`;

  $("#hc-scan").onclick = () => { S.scan = S.startup = S.updates = S.apps = null; S.lastClean = null; healthScan(); VIEWS.health(); };
  $("#hc-clean").onclick = async () => {
    const ids = S.scan.filter((i) => i.default_on && i.bytes > 0 && (!i.admin || S.info?.elevated)).map((i) => i.id);
    $("#hc-clean").disabled = true;
    $("#hc-clean").innerHTML = '<span class="spin"></span> Cleaning...';
    const res = await guard(() => invoke("cleaner_clean", { ids }));
    if (res) {
      S.lastClean = res.reduce((a, r) => a + r.bytes_freed, 0);
      toast(`Moved ${bytes(S.lastClean)} of digital weight.`);
      S.scan = null;
      S.info = await invoke("system_info");
      healthScan();
    }
    if (current === "health") VIEWS.health();
  };
  if (!S.scan && !healthScan.running) healthScan();
};

async function healthScan() {
  if (healthScan.running) return;
  healthScan.running = true;
  const redraw = () => current === "health" && VIEWS.health();
  const jobs = [
    invoke("cleaner_scan").then((r) => { S.scan = r; redraw(); }),
    invoke("startup_list").then((r) => { S.startup = r; redraw(); }),
    invoke("apps_list", { store: false }).then((r) => { S.apps = S.apps || r; redraw(); }),
    invoke("updates_list").then((r) => { if (!S.updates) { S.updates = r; r.forEach((u) => updSel.add(u.id)); } renderNav(); redraw(); }).catch(() => { S.updates = []; redraw(); }),
  ];
  await Promise.allSettled(jobs);
  healthScan.running = false;
}

// ======================================================= CUSTOM CLEAN ====
let cleanTab = "junk";
VIEWS.cleaner = async function () {
  $("#top-actions").innerHTML = "";
  if (!S.scan) {
    $("#page").innerHTML = loading("Measuring junk across every cleaning rule...");
    S.scan = await guard(() => invoke("cleaner_scan"));
    if (!S.scan) return;
    if (current !== "cleaner") { S.scan.forEach((i) => i.default_on && S.checked.add(i.id)); return; }
    S.scan.forEach((i) => i.default_on && S.checked.add(i.id));
  }
  if (!S.checked.size && !VIEWS.cleaner.init) S.scan.forEach((i) => i.default_on && S.checked.add(i.id));
  VIEWS.cleaner.init = true;
  drawCleaner();
};

function drawCleaner() {
  const groups = { junk: "Junk & caches", browser: "Browsers", privacy: "Privacy" };
  const items = S.scan.filter((i) => i.group === cleanTab);
  const cats = [...new Set(items.map((i) => i.category))];
  const sel = S.scan.filter((i) => S.checked.has(i.id));
  const selBytes = sel.reduce((a, i) => a + (i.admin && !S.info?.elevated ? 0 : i.bytes), 0);
  const needAdmin = sel.some((i) => i.admin);

  $("#page").innerHTML = `
    <div class="tabs">${Object.entries(groups)
      .map(([k, v]) => `<button class="tab ${k === cleanTab ? "active" : ""}" data-ctab="${k}">${v}<span class="count">${bytes(S.scan.filter((i) => i.group === k && S.checked.has(i.id)).reduce((a, i) => a + i.bytes, 0))}</span></button>`)
      .join("")}</div>
    ${adminNote(needAdmin)}
    <div class="split">
      <div class="card" style="padding:8px 14px">
        ${cats.map((c) => {
          const its = items.filter((i) => i.category === c);
          const on = its.filter((i) => S.checked.has(i.id)).length;
          return `<div class="cat ${cats.length < 4 ? "open" : ""}" data-cat="${esc(c)}">
            <div class="cat-head"><input type="checkbox" class="cb" data-catcb="${esc(c)}" ${on === its.length ? "checked" : ""}>${esc(c)}<span class="faint" style="font-weight:500">${on}/${its.length}</span><span class="chev">▾</span></div>
            <div class="cat-items">${its.map((i) => `<div class="rule"><input type="checkbox" class="cb" id="r-${esc(i.id)}" data-rule="${esc(i.id)}" ${S.checked.has(i.id) ? "checked" : ""}><label for="r-${esc(i.id)}">${esc(i.name)}${i.admin ? ' <span class="pill admin">admin</span>' : ""}</label><span class="faint">${i.bytes ? bytes(i.bytes) : ""}</span></div>`).join("")}</div>
          </div>`;
        }).join("")}
      </div>
      <div class="card" style="padding:0">
        <div class="scroll"><table class="table"><thead><tr><th></th><th>Item</th><th class="num">Files</th><th class="num">Size</th></tr></thead><tbody>
        ${items.filter((i) => i.bytes > 0 || S.checked.has(i.id)).sort((a, b) => b.bytes - a.bytes).map((i) => `<tr>
          <td style="width:34px"><input type="checkbox" class="cb" data-rule="${esc(i.id)}" ${S.checked.has(i.id) ? "checked" : ""}></td>
          <td><div class="cell-main">${esc(i.category)} - ${esc(i.name)} ${i.admin ? '<span class="pill admin">admin</span>' : ""}</div><div class="cell-sub">${esc(i.description)}${i.warning ? ` <span class="warn">${esc(i.warning)}</span>` : ""}</div></td>
          <td class="num muted">${num(i.files)}</td><td class="num"><b>${bytes(i.bytes)}</b></td></tr>`).join("") || `<tr><td colspan="4"><div class="empty">${icon("shield")}<h3>Nothing to clean here</h3>This section is already light.</div></td></tr>`}
        </tbody></table></div>
      </div>
    </div>
    <div class="footer-bar"><span class="muted">${sel.length} selected</span><b style="font-size:16px">${bytes(selBytes)}</b>
      <button class="btn" id="cl-rescan">Scan again</button>
      <button class="btn primary" id="cl-clean" ${sel.length ? "" : "disabled"}>${icon("weight")}Clean selected</button></div>`;

  $$("[data-ctab]").forEach((b) => (b.onclick = () => { cleanTab = b.dataset.ctab; drawCleaner(); }));
  $$(".cat-head").forEach((h) => (h.onclick = (e) => { if (e.target.matches("input")) return; h.parentElement.classList.toggle("open"); }));
  $$("[data-rule]").forEach((cb) => (cb.onchange = () => { cb.checked ? S.checked.add(cb.dataset.rule) : S.checked.delete(cb.dataset.rule); drawCleaner(); }));
  $$("[data-catcb]").forEach((cb) => (cb.onchange = () => {
    items.filter((i) => i.category === cb.dataset.catcb).forEach((i) => (cb.checked ? S.checked.add(i.id) : S.checked.delete(i.id)));
    drawCleaner();
  }));
  $("#cl-rescan").onclick = () => { S.scan = null; VIEWS.cleaner(); };
  $("#cl-clean").onclick = () => {
    const risky = sel.filter((i) => i.warning || i.id === "win.downloads");
    modal(`<h2>Clean ${sel.length} item${sel.length === 1 ? "" : "s"}?</h2>
      <p class="muted">About <b>${bytes(selBytes)}</b> will be permanently removed. Files that are in use are skipped automatically.</p>
      ${risky.length ? `<div class="note warn-n">Heads up: ${risky.map((i) => `<b>${esc(i.category)} - ${esc(i.name)}</b>`).join(", ")}. ${risky.map((i) => esc(i.warning || "Review before cleaning.")).join(" ")}</div>` : ""}
      <div class="row" style="justify-content:flex-end;margin-top:18px"><button class="btn" data-close>Cancel</button><button class="btn primary" id="cl-go">Clean now</button></div>`);
    $("#cl-go").onclick = async () => {
      $("#modal-card").innerHTML = loading("Moving digital weight...");
      const res = await guard(() => invoke("cleaner_clean", { ids: sel.map((i) => i.id) }));
      if (!res) return closeModal();
      const freed = res.reduce((a, r) => a + r.bytes_freed, 0);
      const skipped = res.reduce((a, r) => a + r.skipped, 0);
      S.lastClean = freed;
      modal(`<h2>Done - <span class="accent">${bytes(freed)}</span> moved</h2>
        <p class="muted">${num(res.reduce((a, r) => a + r.files_deleted, 0))} files removed${skipped ? `, ${num(skipped)} in-use or protected files skipped` : ""}.</p>
        <table class="table"><tbody>${res.sort((a, b) => b.bytes_freed - a.bytes_freed).map((r) => `<tr><td>${esc(r.name)}</td><td class="num">${bytes(r.bytes_freed)}</td><td class="num faint">${r.skipped ? r.skipped + " skipped" : ""}</td></tr>`).join("")}</tbody></table>
        <div class="row" style="justify-content:flex-end;margin-top:18px"><button class="btn primary" data-close>Nice</button></div>`);
      S.scan = null;
      S.info = await invoke("system_info");
      if (current === "cleaner") VIEWS.cleaner();
    };
  };
}

// ======================================================== UNINSTALLER ====
let appFilter = "", appSort = "name", appSel = null, showStore = false;
VIEWS.uninstaller = async function () {
  $("#top-actions").innerHTML = `<label class="row muted" style="gap:6px"><input type="checkbox" class="cb" id="u-store" ${showStore ? "checked" : ""}>Include Store apps</label>
    <input class="search" id="u-q" placeholder="Search programs..." value="${esc(appFilter)}">`;
  $("#u-q").oninput = (e) => { appFilter = e.target.value; drawApps(); };
  $("#u-store").onchange = (e) => { showStore = e.target.checked; S.apps = null; VIEWS.uninstaller(); };
  if (!S.apps || (showStore && !S.apps.some((a) => a.source === "store"))) {
    $("#page").innerHTML = loading("Reading installed programs...");
    S.apps = await guard(() => invoke("apps_list", { store: showStore }));
    if (!S.apps || current !== "uninstaller") return;
  }
  drawApps();
};

function drawApps() {
  const q = appFilter.toLowerCase();
  let list = S.apps.filter((a) => !q || a.name.toLowerCase().includes(q) || a.publisher.toLowerCase().includes(q));
  const key = { name: (a) => a.name.toLowerCase(), size: (a) => -a.size_bytes, date: (a) => (a.install_date ? -Number(a.install_date.replace(/-/g, "")) : 0), publisher: (a) => a.publisher.toLowerCase() }[appSort];
  list = list.slice().sort((a, b) => (key(a) < key(b) ? -1 : key(a) > key(b) ? 1 : 0));
  const total = S.apps.reduce((a, x) => a + x.size_bytes, 0);
  const sel = S.apps.find((a) => a.id === appSel);
  $("#page").innerHTML = `
    <div class="spread" style="margin-bottom:12px"><div class="muted">${num(S.apps.length)} programs - ${bytes(total)} reported by installers</div></div>
    <div class="card" style="padding:0"><div class="scroll"><table class="table">
      <thead><tr><th class="sort" data-sort="name">Program</th><th class="sort" data-sort="publisher">Publisher</th><th>Version</th><th class="sort num" data-sort="date">Installed</th><th class="sort num" data-sort="size">Size</th></tr></thead>
      <tbody>${list.map((a) => `<tr class="click ${a.id === appSel ? "sel" : ""}" data-app="${esc(a.id)}">
        <td><div class="cell-main">${esc(a.name)}</div><div class="cell-sub">${a.source === "store" ? "Microsoft Store app" : a.source === "win32-user" ? "Installed for you only" : esc(a.source)}</div></td>
        <td class="muted">${esc(a.publisher)}</td><td class="mono muted">${esc(a.version)}</td><td class="num muted">${esc(a.install_date)}</td><td class="num">${a.size_bytes ? bytes(a.size_bytes) : ""}</td></tr>`).join("")}
      </tbody></table></div></div>
    <div class="footer-bar">${sel ? `<span><b>${esc(sel.name)}</b> <span class="muted">${esc(sel.version)}</span></span>` : '<span class="muted">Select a program</span>'}
      <button class="btn" id="u-left" ${sel ? "" : "disabled"}>Scan leftovers only</button>
      <button class="btn primary" id="u-go" ${sel && sel.can_uninstall ? "" : "disabled"}>${icon("uninstall")}Uninstall</button></div>`;
  $$("[data-app]").forEach((r) => (r.onclick = () => { appSel = r.dataset.app; drawApps(); }));
  $$("[data-sort]").forEach((h) => (h.onclick = () => { appSort = h.dataset.sort; drawApps(); }));
  if (sel) {
    $("#u-go").onclick = () => uninstallFlow(sel);
    $("#u-left").onclick = () => leftoverFlow(sel, false);
  }
}

function uninstallFlow(app) {
  modal(`<h2>Uninstall ${esc(app.name)}?</h2>
    <p class="muted">MDW runs the program's own uninstaller first, then hunts for leftover folders, shortcuts and registry keys - like Revo's moderate mode.</p>
    <div class="note">Nothing is lost for good: leftovers go to the Recycle Bin and registry keys are backed up as <span class="mono">.reg</span> files before removal.</div>
    <div class="row" style="justify-content:flex-end;margin-top:18px"><button class="btn" data-close>Cancel</button><button class="btn primary" id="un-go">Uninstall</button></div>`);
  $("#un-go").onclick = async () => {
    $("#modal-card").innerHTML = loading(`Running the ${app.name} uninstaller - finish its window if one opens...`);
    const out = await guard(() => invoke("app_uninstall", { app, quiet: false }));
    if (!out) return closeModal();
    if (!out.ok) {
      modal(`<h2>The uninstaller didn't finish</h2><p class="muted">${esc(out.message)}</p>
        <div class="row" style="justify-content:flex-end;margin-top:18px"><button class="btn" data-close>Close</button><button class="btn primary" id="un-force">Scan leftovers anyway</button></div>`);
      $("#un-force").onclick = () => leftoverFlow(app, true);
      return;
    }
    leftoverFlow(app, true, out.message);
  };
}

async function leftoverFlow(app, afterUninstall, msg = "") {
  modal(loading(`Hunting leftovers of ${app.name}...`));
  const items = await guard(() => invoke("app_leftovers", { app }));
  if (!items) return closeModal();
  const chosen = new Set(items.map((_, i) => i));
  const draw = () => {
    const tot = items.filter((_, i) => chosen.has(i)).reduce((a, l) => a + l.bytes, 0);
    modal(`<h2>${afterUninstall ? `${esc(app.name)} removed` : `Leftovers of ${esc(app.name)}`}</h2>
      <p class="muted">${esc(msg)} ${items.length ? `Found ${items.length} leftover item${items.length === 1 ? "" : "s"}.` : "No leftovers found - clean exit."}</p>
      ${!afterUninstall && items.length ? '<div class="note warn-n">This program is still installed. Removing its folders may break it - use this only for broken installs.</div>' : ""}
      ${items.length ? `<div class="scroll" style="max-height:44vh;margin-top:10px"><table class="table"><tbody>${items.map((l, i) => `<tr>
        <td style="width:30px"><input type="checkbox" class="cb" data-lo="${i}" ${chosen.has(i) ? "checked" : ""}></td>
        <td><span class="pill">${l.kind === "regkey" ? "registry" : l.kind}</span></td><td class="mono wrap">${esc(l.path)}</td><td class="num">${l.bytes ? bytes(l.bytes) : ""}</td></tr>`).join("")}</tbody></table></div>` : ""}
      <div class="row" style="justify-content:flex-end;margin-top:18px"><button class="btn" id="lo-close">${items.length ? "Keep all" : "Done"}</button>${items.length ? `<button class="btn primary" id="lo-go" ${chosen.size ? "" : "disabled"}>Remove ${chosen.size} (${bytes(tot)})</button>` : ""}</div>`);
    $$("[data-lo]").forEach((cb) => (cb.onchange = () => { cb.checked ? chosen.add(+cb.dataset.lo) : chosen.delete(+cb.dataset.lo); draw(); }));
    $("#lo-close").onclick = done;
    if ($("#lo-go")) $("#lo-go").onclick = async () => {
      $("#modal-card").innerHTML = loading("Removing leftovers...");
      const rep = await guard(() => invoke("leftovers_remove", { items: items.filter((_, i) => chosen.has(i)) }));
      if (rep) {
        modal(`<h2>Leftovers removed</h2><p class="muted">${rep.removed.length} removed${rep.failed.length ? `, ${rep.failed.length} could not be removed` : ""}.${rep.backup ? ` Registry backups: <span class="mono">${esc(rep.backup)}</span>` : ""}</p>
          ${rep.failed.map(([p, e]) => `<div class="cell-sub"><span class="mono">${esc(p)}</span> - ${esc(e)}</div>`).join("")}
          <div class="row" style="justify-content:flex-end;margin-top:18px"><button class="btn primary" id="lo-ok">Done</button></div>`);
        $("#lo-ok").onclick = done;
      }
    };
  };
  const done = () => { closeModal(); if (afterUninstall) { S.apps = null; appSel = null; if (current === "uninstaller") VIEWS.uninstaller(); } };
  draw();
}

// ============================================================ STARTUP ====
let stTab = "apps";
VIEWS.startup = async function () {
  $("#top-actions").innerHTML = `<button class="btn" id="st-re">Refresh</button>`;
  $("#st-re").onclick = () => { S.startup = null; VIEWS.startup(); };
  if (!S.startup) {
    $("#page").innerHTML = loading("Reading startup entries, scheduled tasks and services...");
    S.startup = await guard(() => invoke("startup_list"));
    if (!S.startup || current !== "startup") return;
  }
  drawStartup();
};

function drawStartup() {
  const tabs = {
    apps: { label: "Startup apps", kinds: ["run", "folder", "autostart", "launchagent"] },
    tasks: { label: "Scheduled tasks", kinds: ["task"] },
    services: { label: "Services", kinds: ["service", "systemd"] },
  };
  const t = tabs[stTab];
  const items = S.startup.filter((i) => t.kinds.includes(i.kind)).sort((a, b) => a.name.localeCompare(b.name));
  const isSvc = stTab === "services";
  $("#page").innerHTML = `
    <div class="tabs">${Object.entries(tabs).map(([k, v]) => `<button class="tab ${k === stTab ? "active" : ""}" data-st="${k}">${v.label}<span class="count">${S.startup.filter((i) => v.kinds.includes(i.kind)).length}</span></button>`).join("")}</div>
    ${adminNote(items.some((i) => i.admin))}
    ${stTab === "apps" ? '<p class="muted" style="margin-top:0">Turning an app off works exactly like Task Manager - nothing is deleted and you can turn it back on any time.</p>' : ""}
    ${isSvc ? '<p class="muted" style="margin-top:0">Third-party services only - Windows\' own services are hidden so you can\'t break the OS by accident.</p>' : ""}
    <div class="card" style="padding:0"><div class="scroll"><table class="table">
      <thead><tr><th>Name</th><th>${isSvc ? "Start mode / state" : "Command"}</th><th style="width:${isSvc ? 170 : 90}px">${isSvc ? "Start mode" : "Starts"}</th></tr></thead>
      <tbody>${items.map((i) => `<tr>
        <td><div class="cell-main">${esc(i.name)}</div><div class="cell-sub">${esc(i.kind === "task" ? i.location : i.scope === "machine" ? "All users" : "You")}${i.admin ? ' <span class="pill admin">admin</span>' : ""}</div></td>
        <td class="${isSvc ? "muted" : "mono muted wrap"}" style="max-width:560px">${esc(isSvc ? i.detail : i.command)}</td>
        <td>${isSvc && i.kind === "service"
          ? `<select class="search" data-svc="${esc(i.location)}">${["Auto", "Manual", "Disabled"].map((m) => `<option ${i.detail.startsWith(m) ? "selected" : ""}>${m}</option>`).join("")}</select>`
          : `<button class="toggle ${i.enabled ? "on" : ""}" data-toggle="${esc(i.id)}" title="${i.enabled ? "Enabled" : "Disabled"}"></button>`}</td></tr>`).join("") || '<tr><td colspan="3" class="muted" style="padding:30px;text-align:center">Nothing here.</td></tr>'}
      </tbody></table></div></div>`;
  $$("[data-st]").forEach((b) => (b.onclick = () => { stTab = b.dataset.st; drawStartup(); }));
  $$("[data-toggle]").forEach((b) => (b.onclick = async () => {
    const item = S.startup.find((i) => i.id === b.dataset.toggle);
    b.disabled = true;
    const r = await guard(() => invoke("startup_set", { id: item.id, enabled: !item.enabled }));
    if (r?.ok) { item.enabled = !item.enabled; toast(`${item.name} ${item.enabled ? "will start" : "won't start"} automatically.`); }
    else if (r) toast(r.message, true);
    drawStartup();
  }));
  $$("[data-svc]").forEach((s) => (s.onchange = async () => {
    const r = await guard(() => invoke("service_mode", { name: s.dataset.svc, mode: s.value }));
    if (r?.ok) { const it = S.startup.find((i) => i.location === s.dataset.svc); it.detail = `${s.value} - ${it.detail.split(" - ")[1] || ""}`; toast(`Service set to ${s.value}.`); }
    else if (r) { toast(r.message, true); drawStartup(); }
  }));
}

// ============================================================ UPDATES ====
// Update state lives outside the page so a run keeps going - and stays
// visible - while you use other tabs.
const updSel = new Set();
const updState = {}; // id -> { s: "queued" | "run" | "ok" | "fail", msg }
let updRunning = false;
const updPending = () => (S.updates || []).filter((u) => updState[u.id]?.s !== "ok");

VIEWS.updates = async function () {
  $("#top-actions").innerHTML = `<button class="btn" id="up-re" ${updRunning ? "disabled" : ""}>Check again</button>`;
  $("#up-re").onclick = () => { S.updates = null; Object.keys(updState).forEach((k) => delete updState[k]); VIEWS.updates(); };
  if (!S.updates) {
    $("#page").innerHTML = loading("Asking winget which apps have updates...");
    const list = await guard(() => invoke("updates_list"));
    if (!list) return;
    S.updates = list;
    S.updates.forEach((u) => updSel.add(u.id));
    renderNav();
    if (current !== "updates") return;
  }
  drawUpdates();
};

function updCell(id) {
  const st = updState[id];
  if (!st) return "";
  if (st.s === "queued") return '<span class="faint">Queued</span>';
  if (st.s === "run") return '<span class="spin"></span> updating';
  if (st.s === "ok") return '<span class="accent">Updated</span>';
  return `<span class="danger-t" title="${esc(st.msg)}">Failed</span>`;
}

function drawUpdates() {
  if (current !== "updates") return;
  const u = S.updates;
  const done = u.filter((x) => updState[x.id]?.s === "ok").length;
  $("#page").innerHTML = u.length
    ? `<p class="muted" style="margin-top:0">Updates come straight from each publisher through Windows Package Manager (winget) - free, no Pro tier. ${updRunning ? "<b>Updates keep running if you switch tabs.</b>" : ""}</p>
      <div class="card" style="padding:0"><div class="scroll"><table class="table">
      <thead><tr><th style="width:34px"><input type="checkbox" class="cb" id="up-all" ${updSel.size === u.length ? "checked" : ""} ${updRunning ? "disabled" : ""}></th><th>Software</th><th>Current</th><th>New version</th><th style="width:130px"></th></tr></thead>
      <tbody>${u.map((x) => `<tr><td><input type="checkbox" class="cb" data-up="${esc(x.id)}" ${updSel.has(x.id) ? "checked" : ""} ${updRunning || updState[x.id]?.s === "ok" ? "disabled" : ""}></td>
        <td><div class="cell-main">${esc(x.name)}</div><div class="cell-sub mono">${esc(x.id)}</div></td><td class="mono muted">${esc(x.current)}</td><td class="mono accent">${esc(x.available)}</td><td>${updCell(x.id)}</td></tr>`).join("")}</tbody></table></div></div>
      <div class="footer-bar"><span class="muted">${updRunning ? `Updating ${done + 1 > u.length ? u.length : done}/${u.length}...` : `${updSel.size} selected`}</span><button class="btn primary" id="up-go" ${updSel.size && !updRunning ? "" : "disabled"}>${updRunning ? '<span class="spin"></span> Updating...' : icon("updates") + "Update selected"}</button></div>`
    : `<div class="empty">${icon("shield")}<h3>Everything is up to date</h3>No outdated apps found.</div>`;
  if (!u.length) return;
  $("#up-all").onchange = (e) => { u.forEach((x) => (e.target.checked && updState[x.id]?.s !== "ok" ? updSel.add(x.id) : updSel.delete(x.id))); drawUpdates(); };
  $$("[data-up]").forEach((cb) => (cb.onchange = () => { cb.checked ? updSel.add(cb.dataset.up) : updSel.delete(cb.dataset.up); drawUpdates(); }));
  $("#up-go").onclick = runUpdates;
}

async function runUpdates() {
  if (updRunning) return;
  const ids = [...updSel];
  updRunning = true;
  ids.forEach((id) => (updState[id] = { s: "queued" }));
  let ok = 0, n = 0;
  for (const id of ids) {
    n++;
    updState[id] = { s: "run" };
    setAct("updates", `Updating ${n}/${ids.length}`);
    drawUpdates();
    const r = await invoke("update_apply", { id }).catch((e) => ({ ok: false, message: String(e) }));
    updState[id] = r.ok ? { s: "ok" } : { s: "fail", msg: r.message };
    if (r.ok) { ok++; updSel.delete(id); }
    drawUpdates();
  }
  updRunning = false;
  setAct("updates", null);
  drawUpdates();
  toast(`${ok} of ${ids.length} app${ids.length === 1 ? "" : "s"} updated.`, ok < ids.length);
}

// ========================================================= DUPLICATES ====
let dupRoot = "", dupMin = 1, dupGroups = null;
const dupDel = new Set();
VIEWS.dupes = function () {
  const home = S.info?.platform === "windows" ? "C:\\Users" : "/home";
  dupRoot = dupRoot || home;
  drawDupes();
};
function drawDupes() {
  const wasted = dupGroups ? dupGroups.reduce((a, g) => a + g.wasted, 0) : 0;
  const delBytes = dupGroups ? dupGroups.reduce((a, g) => a + g.files.filter((f) => dupDel.has(f.path)).length * g.size, 0) : 0;
  $("#page").innerHTML = `
    <div class="card row" style="flex-wrap:wrap">
      <input class="search" id="d-root" style="flex:1" value="${esc(dupRoot)}" placeholder="Folder to search">
      <button class="btn" id="d-pick">${icon("folder")}Browse</button>
      <label class="row muted" style="gap:6px">Min size <select class="search" id="d-min">${[0, 1, 10, 100].map((m) => `<option value="${m}" ${m === dupMin ? "selected" : ""}>${m ? m + " MB" : "any"}</option>`).join("")}</select></label>
      <button class="btn primary" id="d-go">${icon("search")}Find duplicates</button>
    </div>
    <div id="d-res" style="margin-top:14px">${dupGroups === null ? `<div class="empty">${icon("dupes")}<h3>Find identical files</h3>Files are compared by content (BLAKE3 hash), not by name. Windows, AppData and program folders are skipped.</div>` :
      dupGroups.length ? `<div class="spread" style="margin-bottom:10px"><span class="muted">${num(dupGroups.length)} groups - <b>${bytes(wasted)}</b> reclaimable</span><button class="btn small" id="d-auto">Keep oldest copy of each</button></div>
      <div class="card" style="padding:0"><div class="scroll"><table class="table"><tbody>${dupGroups.slice(0, 400).map((g) => `<tr><td colspan="3" style="background:var(--card-2)"><b>${bytes(g.size)}</b> <span class="muted">x ${g.files.length} copies</span></td></tr>${g.files.map((f) => `<tr><td style="width:34px"><input type="checkbox" class="cb" data-dup="${esc(f.path)}" ${dupDel.has(f.path) ? "checked" : ""}></td><td class="mono wrap">${esc(f.path)}</td><td class="num muted">${f.modified ? new Date(f.modified * 1000).toLocaleDateString() : ""}</td></tr>`).join("")}`).join("")}</tbody></table></div></div>
      <div class="footer-bar"><span class="muted">${dupDel.size} files selected</span><b>${bytes(delBytes)}</b><button class="btn primary" id="d-del" ${dupDel.size ? "" : "disabled"}>Move to Recycle Bin</button></div>` : `<div class="empty">${icon("shield")}<h3>No duplicates</h3>Nothing identical in that folder.</div>`}</div>`;
  $("#d-root").oninput = (e) => (dupRoot = e.target.value);
  $("#d-min").onchange = (e) => (dupMin = +e.target.value);
  $("#d-pick").onclick = async () => {
    if (DEMO) return toast("Folder picker works in the desktop app.");
    const p = await TAURI.dialog.open({ directory: true, multiple: false });
    if (p) { dupRoot = p; drawDupes(); }
  };
  $("#d-go").onclick = async () => {
    $("#d-res").innerHTML = loading("Comparing file contents - big folders take a while...");
    setAct("dupes", "Finding duplicates");
    dupGroups = await guard(() => invoke("dupes_find", { roots: [dupRoot], minSize: dupMin * 1048576 }));
    setAct("dupes", null);
    dupDel.clear();
    if (current === "dupes") drawDupes();
  };
  if (!dupGroups?.length) return;
  $("#d-auto").onclick = () => { dupDel.clear(); dupGroups.forEach((g) => g.files.slice(1).forEach((f) => dupDel.add(f.path))); drawDupes(); };
  $$("[data-dup]").forEach((cb) => (cb.onchange = () => {
    const g = dupGroups.find((x) => x.files.some((f) => f.path === cb.dataset.dup));
    if (cb.checked && g.files.every((f) => f.path === cb.dataset.dup || dupDel.has(f.path))) { cb.checked = false; return toast("Keep at least one copy of every file.", true); }
    cb.checked ? dupDel.add(cb.dataset.dup) : dupDel.delete(cb.dataset.dup);
    drawDupes();
  }));
  $("#d-del").onclick = async () => {
    const r = await guard(() => invoke("dupes_remove", { paths: [...dupDel] }));
    if (!r) return;
    toast(`${r.removed.length} duplicates moved to the Recycle Bin${r.failed.length ? `, ${r.failed.length} failed` : ""}.`, !!r.failed.length);
    const gone = new Set(r.removed);
    dupGroups = dupGroups.map((g) => ({ ...g, files: g.files.filter((f) => !gone.has(f.path)) })).filter((g) => g.files.length > 1).map((g) => ({ ...g, wasted: g.size * (g.files.length - 1) }));
    dupDel.clear();
    drawDupes();
  };
}

// ============================================================== DISK ====
let diskRoot = "", diskRep = null;
VIEWS.disk = function () {
  diskRoot = diskRoot || (S.info?.disks?.[0]?.mount ?? "C:\\");
  drawDisk();
};
function drawDisk() {
  const r = diskRep;
  const max = r?.children?.[0]?.bytes || 1;
  $("#page").innerHTML = `
    <div class="card row" style="flex-wrap:wrap">
      ${(S.info?.disks || []).map((d) => `<button class="btn small ${d.mount === diskRoot ? "primary" : ""}" data-drive="${esc(d.mount)}">${esc(d.mount)}</button>`).join("")}
      <input class="search" id="dk-root" style="flex:1" value="${esc(diskRoot)}">
      <button class="btn" id="dk-pick">${icon("folder")}Browse</button>
      <button class="btn primary" id="dk-go">${icon("disk")}Analyze</button>
    </div>
    <div style="margin-top:14px">${!r ? `<div class="empty">${icon("disk")}<h3>Where did the space go?</h3>Pick a drive or folder to see what is heavy.</div>` : `
      <div class="grid g2">
        <div class="card"><div class="spread"><div class="label">Largest folders in ${esc(r.root)}</div><span class="muted">${bytes(r.bytes)} - ${num(r.files)} files</span></div><div class="sep"></div>
          ${r.children.slice(0, 22).map((c) => `<div class="click" data-drill="${esc(c.is_dir ? c.path : "")}" style="margin-bottom:9px;cursor:${c.is_dir ? "pointer" : "default"}"><div class="spread"><span>${c.is_dir ? "📁 " : ""}${esc(c.name)}</span><b>${bytes(c.bytes)}</b></div><div class="bar"><i style="width:${((c.bytes / max) * 100).toFixed(1)}%"></i></div></div>`).join("")}</div>
        <div class="card"><div class="label">Largest files</div><div class="sep"></div>
          ${r.largest_files.slice(0, 22).map((f) => `<div class="spread" style="padding:4px 0"><span class="mono wrap click" data-reveal="${esc(f.path)}" style="cursor:pointer">${esc(f.path)}</span><b style="white-space:nowrap">${bytes(f.bytes)}</b></div>`).join("")}
          <div class="sep"></div><div class="label" style="margin-bottom:8px">By file type</div>
          ${r.by_type.map(([t, b]) => `<span class="pill" style="margin:0 6px 6px 0">.${esc(t)} ${bytes(b)}</span>`).join("")}</div>
      </div>`}</div>`;
  $$("[data-drive]").forEach((b) => (b.onclick = () => { diskRoot = b.dataset.drive; drawDisk(); }));
  $("#dk-root").oninput = (e) => (diskRoot = e.target.value);
  $("#dk-pick").onclick = async () => { if (DEMO) return; const p = await TAURI.dialog.open({ directory: true }); if (p) { diskRoot = p; drawDisk(); } };
  $("#dk-go").onclick = async () => {
    $("#page").lastElementChild.innerHTML = loading(`Measuring ${diskRoot} - a whole drive can take a minute...`);
    setAct("disk", `Analyzing ${diskRoot}`);
    diskRep = await guard(() => invoke("disk_analyze", { root: diskRoot }));
    setAct("disk", null);
    if (current === "disk") drawDisk();
  };
  $$("[data-drill]").forEach((d) => d.dataset.drill && (d.onclick = () => { diskRoot = d.dataset.drill; $("#dk-go").click(); }));
  $$("[data-reveal]").forEach((d) => (d.onclick = () => invoke("reveal", { path: d.dataset.reveal })));
}

// ======================================================= PERFORMANCE ====
VIEWS.perf = async function () {
  $("#top-actions").innerHTML = `<button class="btn" id="pf-re">Refresh</button>`;
  $("#pf-re").onclick = () => VIEWS.perf();
  $("#page").innerHTML = loading("Sampling running programs...");
  const ps = await guard(() => invoke("procs_list"));
  if (!ps || current !== "perf") return;
  const mem = S.info?.memory_total || 1;
  $("#page").innerHTML = `<p class="muted" style="margin-top:0">Heaviest programs right now. To stop one from coming back after a restart, turn it off in <a href="#" data-go="startup">Startup Manager</a>.</p>
    <div class="card" style="padding:0"><div class="scroll"><table class="table"><thead><tr><th>Program</th><th>Impact</th><th class="num">Memory</th><th class="num">CPU</th><th></th></tr></thead>
    <tbody>${ps.slice(0, 120).map((p, i) => `<tr><td><div class="cell-main">${esc(p.name)} ${p.pids.length > 1 ? `<span class="faint">x${p.pids.length}</span>` : ""}</div><div class="cell-sub mono wrap">${esc(p.exe)}</div></td>
      <td><span class="pill ${p.impact.toLowerCase()}">${esc(p.impact)}</span></td><td class="num">${bytes(p.memory)} <span class="faint">${((p.memory / mem) * 100).toFixed(1)}%</span></td><td class="num">${p.cpu.toFixed(1)}%</td>
      <td style="width:90px"><button class="btn small danger" data-kill="${i}">End</button></td></tr>`).join("")}</tbody></table></div></div>`;
  $$("[data-kill]").forEach((b) => (b.onclick = () => {
    const p = ps[+b.dataset.kill];
    modal(`<h2>End ${esc(p.name)}?</h2><p class="muted">Unsaved work in this program will be lost.</p><div class="row" style="justify-content:flex-end"><button class="btn" data-close>Cancel</button><button class="btn danger" id="k-go">End ${p.pids.length} process${p.pids.length > 1 ? "es" : ""}</button></div>`);
    $("#k-go").onclick = async () => { const n = await guard(() => invoke("procs_kill", { pids: p.pids })); closeModal(); toast(`Ended ${n} process${n === 1 ? "" : "es"}.`); if (current === "perf") VIEWS.perf(); };
  }));
};

// =========================================================== DRIVERS ====
VIEWS.drivers = async function () {
  $("#top-actions").innerHTML = `<button class="btn" data-tool="devmgmt">Device Manager</button><button class="btn primary" data-tool="optupdates">${icon("updates")}Windows Update drivers</button>`;
  $("#page").innerHTML = loading("Reading installed drivers...");
  const d = await guard(() => invoke("drivers_list"));
  if (!d || current !== "drivers") return;
  const old = d.filter((x) => x.age_years >= 3).length;
  $("#page").innerHTML = `<div class="note" style="margin-bottom:14px">MDW never downloads drivers from third-party mirrors - that's how "driver updater" tools spread malware and broken drivers. Get driver updates from <b>Windows Update → Optional updates</b> or your PC/GPU maker. ${old ? `<b class="warn">${old} drivers are over 3 years old.</b>` : ""}</div>
    <div class="card" style="padding:0"><div class="scroll"><table class="table"><thead><tr><th>Device</th><th>Provider</th><th>Version</th><th class="num">Date</th></tr></thead>
    <tbody>${d.map((x) => `<tr><td><div class="cell-main">${esc(x.device)}</div><div class="cell-sub">${esc(x.class)}</div></td><td class="muted">${esc(x.provider)}</td><td class="mono muted">${esc(x.version)}</td><td class="num ${x.age_years >= 3 ? "warn" : "muted"}">${esc(x.date)}</td></tr>`).join("")}</tbody></table></div></div>`;
};
document.addEventListener("click", (e) => {
  const t = e.target.closest("[data-tool]");
  if (t) guard(() => invoke("launch_tool", { tool: t.dataset.tool }));
});

// ============================================================ PRIVACY ====
VIEWS.privacy = function () {
  const drives = S.info?.disks || [];
  $("#page").innerHTML = `
    <div class="grid g2">
      <div class="card stack"><div class="row">${icon("shred").replace("<svg", '<svg style="width:28px;height:28px;stroke:var(--danger);fill:none;stroke-width:1.7"')}<h3 style="margin:0">Unrecoverable Delete</h3></div>
        <div class="muted">Overwrites files with random data, then zeros, scrambles the name and deletes them. Recovery tools get nothing back.</div>
        <div class="row"><button class="btn" id="sh-files">Choose files...</button><button class="btn" id="sh-dir">Choose folder...</button></div>
        <div id="sh-list" class="mono muted"></div>
        <div class="row"><label class="muted">Passes <select class="search" id="sh-pass"><option value="1">1 (recommended)</option><option value="3">3</option><option value="7">7</option></select></label>
          <button class="btn danger" id="sh-go" disabled>Shred forever</button></div>
        <div class="faint" style="font-size:12px">On SSDs, wear-levelling can keep old copies of blocks - combine with the free-space wipe below.</div></div>
      <div class="card stack"><div class="row">${icon("shield").replace("<svg", '<svg style="width:28px;height:28px;stroke:var(--accent-2);fill:none;stroke-width:1.7"')}<h3 style="margin:0">Evidence Remover - wipe free space</h3></div>
        <div class="muted">Files you already deleted normally still sit in "free" space until overwritten. This overwrites all free space on a drive (Windows uses the built-in <span class="mono">cipher /w</span>).</div>
        <div class="row"><select class="search" id="wf-drive">${drives.map((d) => `<option value="${esc(d.mount)}">${esc(d.mount)} - ${bytes(d.free)} free</option>`).join("")}</select>
          <button class="btn primary" id="wf-go">Wipe free space</button></div>
        <div class="faint" style="font-size:12px">Takes a long time on big drives and briefly fills the disk. Your existing files are not touched.</div></div>
    </div>`;
  let picked = [];
  const show = () => { $("#sh-list").innerHTML = picked.map(esc).join("<br>"); $("#sh-go").disabled = !picked.length; };
  $("#sh-files").onclick = async () => { if (DEMO) return; const p = await TAURI.dialog.open({ multiple: true }); if (p) { picked = [].concat(p); show(); } };
  $("#sh-dir").onclick = async () => { if (DEMO) return; const p = await TAURI.dialog.open({ directory: true }); if (p) { picked = [p]; show(); } };
  $("#sh-go").onclick = () => {
    modal(`<h2 class="danger-t">Destroy ${picked.length} item${picked.length > 1 ? "s" : ""} forever?</h2><p class="muted">This cannot be undone - shredded files skip the Recycle Bin and cannot be recovered by anyone, including you.</p>
      <div class="mono muted">${picked.map(esc).join("<br>")}</div>
      <div class="row" style="justify-content:flex-end;margin-top:18px"><button class="btn" data-close>Cancel</button><button class="btn danger" id="sh-yes">Shred forever</button></div>`);
    $("#sh-yes").onclick = async () => {
      $("#modal-card").innerHTML = loading("Shredding...");
      const r = await guard(() => invoke("shred_paths", { paths: picked, passes: +$("#sh-pass").value }));
      closeModal();
      if (r) toast(`Shredded ${r.files} files (${bytes(r.bytes)})${r.failed.length ? `, ${r.failed.length} failed` : ""}.`, !!r.failed.length);
      picked = []; show();
    };
  };
  $("#wf-go").onclick = () => {
    const d = $("#wf-drive").value;
    modal(`<h2>Wipe free space on ${esc(d)}?</h2><p class="muted">This can take an hour or more on a large drive. You can keep using the PC, but it will be slower.</p>
      <div class="row" style="justify-content:flex-end;margin-top:18px"><button class="btn" data-close>Cancel</button><button class="btn primary" id="wf-yes">Start wipe</button></div>`);
    $("#wf-yes").onclick = async () => {
      closeModal();
      toast(`Wiping free space on ${d}... MDW will tell you when it's finished.`);
      setAct("privacy", `Wiping free space on ${d}`);
      const r = await guard(() => invoke("wipe_free", { dir: d }));
      setAct("privacy", null);
      if (r) toast(r);
    };
  };
};

// ============================================================== TOOLS ====
VIEWS.tools = function () {
  const tools = [
    ["cleanmgr", "Disk Cleanup", "Windows' own cleaner, incl. old Windows installs"],
    ["storage", "Storage Sense", "Automatic cleanup settings"],
    ["restore", "System Restore", "Roll back system changes"],
    ["devmgmt", "Device Manager", "Hardware and drivers"],
    ["services", "Services", "Every Windows service"],
    ["taskschd", "Task Scheduler", "All scheduled tasks"],
    ["eventvwr", "Event Viewer", "System logs and errors"],
    ["diskmgmt", "Disk Management", "Partitions and volumes"],
    ["msinfo", "System Information", "Full hardware report"],
    ["resmon", "Resource Monitor", "Live CPU, disk, network"],
    ["taskmgr", "Task Manager", "Processes and startup"],
    ["sysprops", "System Properties", "Performance, env vars"],
    ["defrag", "Optimize Drives", "TRIM / defragment"],
    ["apps", "Apps & features", "Windows' uninstall list"],
    ["winupdate", "Windows Update", "OS updates"],
    ["regedit", "Registry Editor", "For experts"],
  ];
  $("#page").innerHTML = `<p class="muted" style="margin-top:0">Shortcuts to the tools already built into Windows.</p>
    <div class="grid g4">${tools.map(([k, n, d]) => `<div class="card tool" data-tool="${k}">${icon("tools")}<b>${esc(n)}</b><span class="muted" style="font-size:12.5px">${esc(d)}</span></div>`).join("")}</div>`;
};

// ============================================================== ABOUT ====
VIEWS.about = function () {
  const i = S.info || {};
  $("#page").innerHTML = `<div class="grid g2">
    <div class="card stack"><div class="row"><svg class="logo" viewBox="0 0 64 64" style="width:54px;height:54px">${$(".logo").innerHTML}</svg><div><div class="big">MDW</div><div class="muted">Move Digital Weight - v${esc(i.version || "")}</div></div></div>
      <div>Free and open source. No ads, no "Pro" upsell, no telemetry. MDW replaces CCleaner and Revo Uninstaller with one lightweight app.</div>
      <div class="muted">License: GPL-3.0-or-later - Made by the MoveWeight Foundation.</div>
      <div class="row">${i.elevated ? '<span class="pill low">Running as administrator</span>' : '<button class="btn" data-admin>Restart as administrator</button>'}</div></div>
    <div class="card"><div class="label" style="margin-bottom:8px">This machine</div>
      <table class="table"><tbody>
        <tr><td class="muted">Computer</td><td>${esc(i.hostname)}</td></tr>
        <tr><td class="muted">System</td><td>${esc(i.os)}</td></tr>
        <tr><td class="muted">Processor</td><td>${esc(i.cpu)} (${esc(i.cores)} threads)</td></tr>
        <tr><td class="muted">Memory</td><td>${bytes(i.memory_used)} used of ${bytes(i.memory_total)}</td></tr>
        <tr><td class="muted">Up since</td><td>${i.uptime_secs ? new Date(Date.now() - i.uptime_secs * 1000).toLocaleString() : ""}</td></tr>
      </tbody></table></div>
    <div class="card" style="grid-column:1/-1"><div class="label" style="margin-bottom:8px">How MDW keeps you safe</div>
      <ul class="muted" style="margin:0;padding-left:18px;line-height:1.8">
        <li>Scans never delete anything. Cleaning only touches caches and temp files, and skips anything in use.</li>
        <li>Uninstall leftovers go to the Recycle Bin; registry keys are exported to <span class="mono">.reg</span> backups before removal.</li>
        <li>Startup items are switched off the same way Task Manager does it - never deleted.</li>
        <li>Updates come from winget (the publisher's own installers); drivers only from Windows Update or the maker.</li>
      </ul></div></div>`;
};

// ============================================================== boot ====
(async function boot() {
  renderNav();
  try {
    S.info = await invoke("system_info");
  } catch (e) {
    S.info = { disks: [], elevated: false };
  }
  $("#elev").className = "elev" + (S.info.elevated ? " admin" : "");
  $("#elev").innerHTML = `<span class="dot"></span>${S.info.elevated ? "Administrator" : `Standard user - <a href="#" data-admin>elevate</a>`}`;
  $("#ver").textContent = `v${S.info.version || ""}${DEMO ? " - web demo" : ""} - ${S.info.hostname || ""}`;
  go("health");
})();
