/* MDW Tech Toolkit - the Geek Squad MRI / Hiren's-style bench: system report,
   repairs, security, network, crashes. Repairs are engine jobs, so they keep
   running (and stay visible in the sidebar) while you use other pages. */
"use strict";

const TK = { report: null, security: null, events: null, network: null, wifi: null, tasks: null, taskJobs: {}, open: null, reveal: false };

const kv = (rows) => `<table class="table kv"><tbody>${rows.filter(([, v]) => v !== undefined && v !== null && v !== "").map(([k, v]) => `<tr><td class="muted">${esc(k)}</td><td>${v}</td></tr>`).join("")}</tbody></table>`;
const arr = (v) => (Array.isArray(v) ? v : v ? [v] : []);
const okPill = (ok, yes = "OK", no = "Problem") => `<span class="pill ${ok ? "low" : "high"}">${ok ? yes : no}</span>`;
function maskKey(k) { return !k ? "" : TK.reveal ? `<span class="mono">${esc(k)}</span>` : `<span class="mono">${esc(k.replace(/[A-Z0-9](?=[A-Z0-9-]{5})/g, "•"))}</span>`; }

async function tkLoad(key, cmd, msg, page) {
  if (TK[key]) return true;
  $("#page").innerHTML = loading(msg);
  const r = await guard(() => invoke(cmd));
  if (!r) return false;
  TK[key] = r;
  return current === page;
}
function refreshBtn(key, page) {
  $("#top-actions").insertAdjacentHTML("afterbegin", `<button class="btn" id="tk-re">Refresh</button>`);
  $("#tk-re").onclick = () => { TK[key] = null; VIEWS[page](); };
}

// -------------------------------------------------------- System report ----
VIEWS.report = async function () {
  $("#top-actions").innerHTML = `<button class="btn primary" id="tk-export">${icon("report")}Export report</button>`;
  refreshBtn("report", "report");
  if (!(await tkLoad("report", "toolkit_report", "Reading hardware, BIOS, license and disk health...", "report"))) return;
  drawReport();
  $("#tk-export").onclick = exportReport;
};

function drawReport() {
  const r = TK.report;
  const ram = arr(r.Memory);
  const disks = arr(r.Disks);
  const gpus = arr(r.Gpus);
  const bat = arr(r.Battery);
  $("#page").innerHTML = `<div class="grid g2">
    <div class="card"><div class="label">Computer</div>${kv([
      ["Maker / model", esc(`${r.Manufacturer || ""} ${r.Model || ""}`.trim())],
      ["Serial number", esc(r.Serial)],
      ["Motherboard", esc(r.Board)],
      ["BIOS", esc(`${r.BiosVendor || ""} ${r.BiosVersion || ""}${r.BiosDate ? ` (${r.BiosDate})` : ""}`.trim())],
      ["Type", esc(r.SystemType)],
    ])}</div>
    <div class="card"><div class="spread"><div class="label">Windows &amp; license</div><button class="btn small" id="tk-reveal">${TK.reveal ? "Hide keys" : "Show keys"}</button></div>${kv([
      ["Edition", esc(r.Os)],
      ["Version", esc(`${r.OsVersion || ""}${r.Build ? ` (build ${r.Build})` : ""}`)],
      ["Installed", esc(r.InstallDate)],
      ["Last boot", esc(r.LastBoot)],
      ["Activation", r.Activation ? `<span class="pill ${r.Activation === "Activated" ? "low" : "high"}">${esc(r.Activation)}</span> <span class="faint">${esc(r.LicenseName || "")}</span>` : ""],
      ["Product key (last 5)", r.PartialKey ? `<span class="mono">*****-${esc(r.PartialKey)}</span>` : ""],
      ["OEM key in BIOS", r.OemKey ? maskKey(r.OemKey) : r.Os ? '<span class="faint">none (retail / digital license)</span>' : ""],
    ])}</div>
    <div class="card"><div class="label">Processor &amp; memory</div>${kv([
      ["CPU", esc(r.Cpu)],
      ["Cores / threads", r.Cores ? `${r.Cores} / ${r.Threads}` : esc(r.Threads)],
      ["Max clock", r.MaxMHz ? `${(r.MaxMHz / 1000).toFixed(2)} GHz` : ""],
      ["Installed RAM", r.RamBytes ? bytes(r.RamBytes) : ""],
    ])}${ram.length ? `<table class="table" style="margin-top:8px"><thead><tr><th>Slot</th><th>Size</th><th>Speed</th><th>Part</th></tr></thead><tbody>${ram.map((m) => `<tr><td>${esc(m.Slot)}</td><td>${bytes(m.Size)}</td><td>${m.Speed ? m.Speed + " MT/s" : ""}</td><td class="mono muted">${esc(m.Maker)} ${esc(m.Part)}</td></tr>`).join("")}</tbody></table>` : ""}</div>
    <div class="card"><div class="label">Graphics &amp; battery</div>${gpus.map((g) => kv([["GPU", esc(g.Name)], ["Driver", esc(g.Driver)]])).join("")}
      ${bat.length ? bat.map((b) => kv([["Battery", esc(b.Name)], ["Charge", b.Charge != null ? `${b.Charge}%` : ""]])).join("") + `<button class="btn small" data-task="battery" style="margin-top:8px">Full battery health report</button>` : '<div class="muted" style="margin-top:8px">No battery (desktop).</div>'}</div>
    <div class="card" style="grid-column:1/-1"><div class="label">Drives &amp; health</div>
      <table class="table"><thead><tr><th>Drive</th><th>Type</th><th class="num">Size</th><th>Health</th><th class="num">Temp</th><th class="num">Wear</th><th class="num">Power-on</th><th class="num">Errors (r/w)</th></tr></thead><tbody>
      ${disks.map((d) => `<tr><td class="cell-main">${esc(d.Name)}</td><td class="muted">${esc(d.Media)} ${esc(d.Bus || "")}</td><td class="num">${d.Size ? bytes(d.Size) : ""}</td>
        <td>${d.Health ? `<span class="pill ${d.Health === "Healthy" ? "low" : "high"}">${esc(d.Health)}</span>` : ""}</td>
        <td class="num">${d.Temp ? `${d.Temp}°C` : '<span class="faint">-</span>'}</td><td class="num">${d.Wear != null ? `${d.Wear}%` : '<span class="faint">-</span>'}</td>
        <td class="num">${d.PowerOnHours ? `${num(d.PowerOnHours)} h` : '<span class="faint">-</span>'}</td><td class="num">${d.ReadErrors != null ? `${d.ReadErrors} / ${d.WriteErrors ?? 0}` : '<span class="faint">-</span>'}</td></tr>`).join("")}
      </tbody></table>${!S.info?.elevated && disks.some((d) => d.Temp == null) ? adminNote(true).replace("Some items", "Temperature, wear and error counters") : ""}</div>
  </div>`;
  $("#tk-reveal").onclick = () => { TK.reveal = !TK.reveal; drawReport(); };
}

async function exportReport() {
  const r = TK.report;
  if (!r) return;
  const [sec, ev] = await Promise.all([TK.security || invoke("toolkit_security").catch(() => null), TK.events || invoke("toolkit_events").catch(() => null)]);
  TK.security = TK.security || sec;
  TK.events = TK.events || ev;
  const rows = (o) => Object.entries(o).filter(([, v]) => v !== null && typeof v !== "object").map(([k, v]) => `<tr><th>${esc(k)}</th><td>${esc(v)}</td></tr>`).join("");
  const table = (list) => { const a = arr(list); if (!a.length) return "<p>None</p>"; const ks = Object.keys(a[0]); return `<table><tr>${ks.map((k) => `<th>${esc(k)}</th>`).join("")}</tr>${a.map((x) => `<tr>${ks.map((k) => `<td>${esc(typeof x[k] === "object" ? JSON.stringify(x[k]) : x[k])}</td>`).join("")}</tr>`).join("")}</table>`; };
  const html = `<!doctype html><html><head><meta charset="utf-8"><title>MDW System Report - ${esc(S.info?.hostname || "")}</title>
<style>body{font:14px/1.5 Segoe UI,system-ui,sans-serif;margin:32px;color:#1b2232}h1{margin:0}h2{margin-top:28px;border-bottom:2px solid #5ee08f}table{border-collapse:collapse;margin:8px 0;width:100%}th,td{border:1px solid #d7dbe4;padding:5px 8px;text-align:left;vertical-align:top}th{background:#f2f4f8}</style></head><body>
<h1>MDW System Report</h1><p>${esc(S.info?.hostname || "")} - ${new Date().toLocaleString()} - MDW v${esc(S.info?.version || "")}</p>
<h2>System</h2><table>${rows({ ...r, OemKey: r.OemKey ? "(present - hidden in exported report)" : "" })}</table>
<h2>Memory</h2>${table(r.Memory)}<h2>Graphics</h2>${table(r.Gpus)}<h2>Drives</h2>${table(r.Disks)}<h2>Battery</h2>${table(r.Battery)}
${sec ? `<h2>Security</h2><table>${rows(sec)}</table><h3>Firewall</h3>${table(sec.Firewall)}<h3>Threat history</h3>${table(sec.Threats)}` : ""}
${ev ? `<h2>Crashes / unexpected shutdowns (60 days)</h2>${table(ev.Crashes)}<h2>Recent errors (7 days)</h2>${table(arr(ev.Errors).slice(0, 80))}` : ""}
</body></html>`;
  if (DEMO) return toast("Export works in the desktop app.");
  const path = await TAURI.dialog.save({ defaultPath: `MDW-Report-${S.info?.hostname || "PC"}-${new Date().toISOString().slice(0, 10)}.html`, filters: [{ name: "HTML", extensions: ["html"] }] });
  if (!path) return;
  if (await guard(() => invoke("files_write", { path, text: html }).then(() => true))) {
    toast("Report saved.");
    invoke("open_default", { path });
  }
}

// ---------------------------------------------------------------- Repair ----
async function ensureTasks() {
  if (!TK.tasks) TK.tasks = (await guard(() => invoke("toolkit_tasks"))) || [];
}
function taskJob(id) {
  const jid = TK.taskJobs[id];
  return jid ? JOBS.list.find((j) => j.id === jid) : null;
}
async function startTask(id) {
  await ensureTasks();
  const t = TK.tasks.find((x) => x.id === id);
  if (!t) return;
  if (taskJob(id)?.state === "running") return;
  if (t.warning) {
    const ok = await choose(esc(t.name), `<p class="muted">${esc(t.warning)}</p>`, [["go", "Run it", "primary"]]);
    if (!ok) return;
  }
  const jid = await guard(() => invoke("toolkit_run", { id }));
  if (!jid) return;
  TK.taskJobs[id] = jid;
  TK.open = id;
  watchJob(jid, () => { if (current === "repair") drawRepair(); });
  if (current === "repair") drawRepair();
}
document.addEventListener("click", (e) => {
  const b = e.target.closest("[data-task]");
  if (b) startTask(b.dataset.task);
});

VIEWS.repair = async function () {
  $("#top-actions").innerHTML = "";
  await ensureTasks();
  if (current === "repair") drawRepair();
};

function drawRepair() {
  const groups = [...new Set(TK.tasks.map((t) => t.group))];
  $("#page").innerHTML = `${adminNote(TK.tasks.some((t) => t.admin)).replace("Some items need administrator rights.", "Repairs marked <b>admin</b> will ask for permission (UAC) - or")}
    <p class="muted" style="margin-top:0">The same fixes a Geek Squad bench runs, using the tools built into ${isWin() ? "Windows" : "your system"}. They keep running if you switch pages - watch the sidebar.</p>
    ${groups.map((g) => `<div class="label" style="margin:18px 0 8px">${esc(g)}</div><div class="grid g2">${TK.tasks.filter((t) => t.group === g).map(taskCard).join("")}</div>`).join("")}`;
  $$("[data-toggle-out]").forEach((b) => (b.onclick = () => { TK.open = TK.open === b.dataset.toggleOut ? null : b.dataset.toggleOut; drawRepair(); }));
}

function taskCard(t) {
  const j = taskJob(t.id);
  const running = j?.state === "running";
  const status = !j ? "" : running ? `<span class="pill medium"><span class="spin sm"></span> running</span>` : j.state === "done" ? '<span class="pill low">done</span>' : `<span class="pill high">${esc(j.state)}</span>`;
  const out = j && TK.open === t.id ? `<pre class="console mono">${esc(j.output.slice(-400).join("\n"))}${running && j.current ? "\n" + esc(j.current) : ""}</pre>${j.message ? `<div class="${j.state === "done" ? "accent" : "danger-t"}" style="margin-top:6px">${esc(j.message)}</div>` : ""}` : "";
  return `<div class="card task"><div class="spread"><div><b>${esc(t.name)}</b> ${t.admin ? '<span class="pill admin">admin</span>' : ""} ${status}</div><span class="faint">~${esc(t.minutes)} min</span></div>
    <div class="muted" style="margin:6px 0 10px">${esc(t.description)}</div>
    <div class="row"><button class="btn small ${running ? "" : "primary"}" data-task="${t.id}" ${running ? "disabled" : ""}>${running ? "Running..." : j ? "Run again" : "Run"}</button>
      ${running ? `<button class="btn small danger" data-cancel="${j.id}">Stop</button>` : ""}
      ${j ? `<button class="btn small ghost" data-toggle-out="${t.id}">${TK.open === t.id ? "Hide output" : "Show output"}</button>` : ""}</div>${out}</div>`;
}
document.addEventListener("mdw-jobs", () => { if (current === "repair" && TK.tasks) drawRepair(); });

// -------------------------------------------------------------- Security ----
VIEWS.security = async function () {
  $("#top-actions").innerHTML = `<button class="btn" data-task="defupdate">Update definitions</button><button class="btn primary" data-task="defquick">${icon("shield")}Quick scan</button>`;
  refreshBtn("security", "security");
  if (!(await tkLoad("security", "toolkit_security", "Checking Defender, firewall and encryption...", "security"))) return;
  drawSecurity();
};

function drawSecurity() {
  const s = TK.security;
  const fw = arr(s.Firewall);
  const bl = arr(s.BitLocker);
  const threats = arr(s.Threats);
  $("#page").innerHTML = `
    <div class="grid g4">
      <div class="card"><div class="label">Real-time protection</div><div class="stat v">${s.Available ? okPill(s.RealTime, "On", "Off") : '<span class="pill">n/a</span>'}</div><div class="muted">Microsoft Defender</div></div>
      <div class="card"><div class="label">Definitions</div><div class="v" style="font-size:16px;margin:8px 0">${esc(s.SigUpdated || "-")}</div><div class="muted mono">${esc(s.SigVersion || "")}</div></div>
      <div class="card"><div class="label">Last scans</div><div class="v" style="font-size:16px;margin:8px 0">${s.QuickScanAge != null ? `quick ${s.QuickScanAge}d ago` : "-"}</div><div class="muted">${s.FullScanAge != null && s.FullScanAge < 4000 ? `full ${s.FullScanAge}d ago` : "no full scan yet"}</div></div>
      <div class="card"><div class="label">Firewall</div><div style="margin:8px 0">${fw.map((f) => `${esc(f.Name)} ${okPill(f.Enabled, "on", "off")}`).join(" ") || "-"}</div><div class="muted">UAC ${s.UAC === 1 ? "on" : s.UAC === 0 ? '<span class="danger-t">off</span>' : "-"}${s.Tamper != null ? ` - tamper protection ${s.Tamper ? "on" : "off"}` : ""}</div></div>
    </div>
    <div class="grid g2" style="margin-top:14px">
      <div class="card"><div class="spread"><div class="label">Threat history</div><button class="btn small" data-task="deffull">Full scan</button></div>
        ${threats.length ? `<table class="table"><thead><tr><th>When</th><th>Threat</th><th>Cleaned</th></tr></thead><tbody>${threats.map((t) => `<tr><td class="muted">${esc(t.When)}</td><td><b>${esc(t.Name || t.ThreatID)}</b><div class="cell-sub mono wrap">${esc(t.Resources)}</div></td><td>${okPill(t.Cleaned, "yes", "no")}</td></tr>`).join("")}</tbody></table>` : `<div class="empty" style="padding:30px">${icon("shield")}<h3>No threats found</h3>Defender has no detections on record.</div>`}</div>
      <div class="card"><div class="label">Drive encryption (BitLocker)</div>${bl.length ? kv(bl.map((b) => [b.Drive, b.Protection === 1 ? '<span class="pill low">protected</span>' : '<span class="pill">off</span>'])) : `<div class="muted" style="margin-top:8px">${S.info?.elevated ? "No encryptable volumes reported." : "Needs administrator to read."}</div>`}
        <div class="sep"></div>
        <div class="spread"><div class="label">Saved Wi-Fi networks</div><button class="btn small" id="wifi-go">${TK.wifi ? "Refresh" : "Show"}</button></div>
        <div id="wifi-box" class="muted" style="margin-top:8px">${TK.wifi ? wifiTable() : "Lists networks this PC remembers and their passwords - handy when a new device needs the Wi-Fi key."}</div></div>
    </div>`;
  $("#wifi-go").onclick = async () => {
    $("#wifi-box").innerHTML = '<span class="spin"></span>';
    TK.wifi = (await guard(() => invoke("toolkit_wifi"))) || [];
    if (current === "security") $("#wifi-box").innerHTML = wifiTable();
    wireWifi();
  };
  wireWifi();
}
function wifiTable() {
  const w = arr(TK.wifi);
  if (!w.length) return "No saved Wi-Fi networks.";
  return `<table class="table"><tbody>${w.map((n, i) => `<tr><td class="cell-main">${esc(n.Name)}</td><td class="muted">${esc(n.Auth)}</td><td class="mono">${n.Key ? `<span data-wk="${i}" class="click" style="cursor:pointer">••••••••</span>` : `<span class="faint">${S.info?.elevated ? "open / none" : "needs admin"}</span>`}</td></tr>`).join("")}</tbody></table>`;
}
function wireWifi() {
  $$("[data-wk]").forEach((s) => (s.onclick = () => { s.textContent = s.textContent.startsWith("•") ? arr(TK.wifi)[+s.dataset.wk].Key : "••••••••"; }));
}

// --------------------------------------------------------------- Network ----
VIEWS.network = async function () {
  $("#top-actions").innerHTML = `<button class="btn" data-task="dns">Flush DNS</button><button class="btn" data-task="renew">Renew IP</button><button class="btn danger" data-task="netreset">Reset network</button>`;
  refreshBtn("network", "network");
  if (!(await tkLoad("network", "toolkit_network", "Testing router, internet, DNS and HTTPS...", "network"))) return;
  const n = TK.network;
  const tests = arr(n.Tests);
  const allOk = tests.every((t) => t.Ok);
  const firstBad = tests.find((t) => !t.Ok);
  const hint = !firstBad ? "Everything answers. If a site is still slow, it's probably that site." :
    firstBad.Name.startsWith("Router") ? "Your PC can't reach the router - check Wi-Fi / cable, then try Renew IP." :
    firstBad.Name.startsWith("Internet") ? "The router answers but the internet doesn't - restart the modem/router or call your ISP." :
    firstBad.Name.startsWith("DNS") ? "Internet works but names don't resolve - try Flush DNS, or set DNS to 1.1.1.1." :
    "Secure web traffic is blocked - a firewall, VPN or proxy may be interfering.";
  $("#page").innerHTML = `<div class="card hero" style="padding:20px 24px"><div style="font-size:40px">${allOk ? "✅" : "⚠️"}</div><div><h2 style="margin:0 0 4px">${allOk ? "Connection looks healthy" : "Found a network problem"}</h2><div class="muted">${esc(hint)}</div></div></div>
    <div class="grid g4" style="margin-top:14px">${tests.map((t) => `<div class="card"><div class="label">${esc(t.Name)}</div><div style="margin:8px 0">${okPill(t.Ok, "pass", "fail")}</div><div class="muted mono" style="font-size:12px">${esc(t.Target)} ${esc(t.Detail)}</div></div>`).join("")}</div>
    <div class="card" style="margin-top:14px;padding:0"><table class="table"><thead><tr><th>Adapter</th><th>IPv4</th><th>Gateway</th><th>DNS</th><th>Speed</th><th>MAC</th></tr></thead><tbody>
    ${arr(n.Adapters).map((a) => `<tr><td><div class="cell-main">${esc(a.Name)}</div><div class="cell-sub">${esc(a.Desc)}</div></td><td class="mono">${esc(a.IPv4)}</td><td class="mono">${esc(a.Gateway)}</td><td class="mono muted">${esc(a.DNS)}</td><td>${esc(a.Speed)}</td><td class="mono muted">${esc(a.Mac)}</td></tr>`).join("") || '<tr><td colspan="6" class="muted">No active adapters.</td></tr>'}</tbody></table></div>`;
};

// ---------------------------------------------------------------- Events ----
VIEWS.events = async function () {
  $("#top-actions").innerHTML = `<input class="search" id="ev-q" placeholder="Filter errors...">`;
  refreshBtn("events", "events");
  if (!(await tkLoad("events", "toolkit_events", "Reading crash history and error logs...", "events"))) return;
  const draw = (q = "") => {
    const e = TK.events;
    const crashes = arr(e.Crashes);
    const errs = arr(e.Errors).filter((x) => !q || `${x.Source} ${x.Message} ${x.Id}`.toLowerCase().includes(q));
    const kinds = { 41: "Unexpected power loss / hard reset", 1001: "Blue screen (bugcheck)", 6008: "Unexpected shutdown" };
    const bySource = {};
    arr(e.Errors).forEach((x) => (bySource[x.Source] = (bySource[x.Source] || 0) + 1));
    const top = Object.entries(bySource).sort((a, b) => b[1] - a[1]).slice(0, 6);
    $("#page").innerHTML = `<div class="grid g2">
      <div class="card"><div class="label">Crashes &amp; unexpected shutdowns (60 days)</div>${crashes.length ? `<table class="table"><tbody>${crashes.map((c) => `<tr><td class="muted" style="white-space:nowrap">${esc(c.Time)}</td><td><b>${esc(kinds[c.Id] || c.Source)}</b><div class="cell-sub">${esc(c.Message)}</div></td></tr>`).join("")}</tbody></table>` : '<div class="muted" style="margin-top:8px">None - no blue screens or hard resets recorded.</div>'}
        ${arr(e.Dumps).length ? `<div class="sep"></div><div class="label">Crash dump files</div>${arr(e.Dumps).map((d) => `<div class="spread"><span class="mono">${esc(d.Name)}</span><span class="muted">${esc(d.Time)} - ${bytes(d.Size)}</span></div>`).join("")}` : ""}</div>
      <div class="card"><div class="label">Noisiest error sources (7 days)</div>${top.map(([s, n]) => `<div class="spread" style="padding:4px 0"><span>${esc(s)}</span><b>${n}</b></div>`).join("") || '<div class="muted">No errors logged.</div>'}</div>
    </div>
    <div class="card" style="margin-top:14px;padding:0"><div class="scroll"><table class="table"><thead><tr><th style="width:140px">Time</th><th>Source</th><th style="width:70px">ID</th><th>Message</th></tr></thead><tbody>
      ${errs.map((x) => `<tr><td class="muted">${esc(x.Time)}</td><td>${esc(x.Source)}<div class="cell-sub">${esc(x.Log || "")}</div></td><td class="mono">${esc(x.Id)}</td><td class="wrap">${esc(x.Message)}</td></tr>`).join("") || '<tr><td colspan="4" class="muted" style="padding:24px;text-align:center">Nothing matches.</td></tr>'}</tbody></table></div></div>`;
  };
  draw();
  $("#ev-q").oninput = (e) => draw(e.target.value.toLowerCase());
};
