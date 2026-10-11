/* MWM Rescue - the 2026 MRI replacement: tool catalog, FMOD-style
   integrity scanning, and CPU-Z/GPU-Z style hardware inspection.
   One page on every host: desktop app, server web UI, static demo. */
"use strict";

const RESCUE = { tab: "hardware", catalog: null, integrity: null, hw: null, scanJob: null };

VIEWS.rescue = async function () {
  $("#top-actions").innerHTML = '<button class="btn" id="rs-re">Refresh</button>';
  $("#rs-re").onclick = () => { RESCUE.catalog = null; RESCUE.integrity = null; RESCUE.hw = null; VIEWS.rescue(); };
  $("#page").innerHTML = loading("Loading rescue center...");
  const [cat, integ] = await Promise.all([
    guard(() => invoke("rescue_catalog")).catch(() => null),
    guard(() => invoke("integrity_status")).catch(() => null),
  ]);
  if (current !== "rescue") return;
  RESCUE.catalog = cat; RESCUE.integrity = integ;
  drawRescue();
};

function drawRescue() {
  if (current !== "rescue") return;
  const tools = arr(RESCUE.catalog?.tools);
  const cats = [...new Set(tools.map((t) => t.category))];
  const tabs = [["hardware", "Hardware (CPU-Z/GPU-Z)"], ["integrity", `Integrity (FMOD)${RESCUE.integrity?.available ? " · baselined" : ""}`], ["catalog", `Tool catalog · ${tools.length}`]];
  $("#page").innerHTML = `<div class="cc-hero card"><div><div class="label">MWM Rescue · 2026</div><h2>Rescue Center</h2>
    <p>The Geek Squad MRI disc, redone: every function is now built into MWM or replaced with a current, legal tool. Desktop, server, WinPE and Linux rescue all read this same catalog.</p></div></div>
    <div class="tabs cc-tabs">${tabs.map(([id, label]) => `<button class="tab ${RESCUE.tab === id ? "active" : ""}" data-rs-tab="${id}">${esc(label)}</button>`).join("")}</div>
    <div id="rs-body"></div>`;
  $$("[data-rs-tab]").forEach((b) => (b.onclick = () => { RESCUE.tab = b.dataset.rsTab; drawRescue(); }));
  ({ hardware: drawRsHardware, integrity: drawRsIntegrity, catalog: drawRsCatalog }[RESCUE.tab] || drawRsHardware)();
}

// ------------------------------------------------------- hardware tab ----
function drawRsHardware() {
  const box = $("#rs-body");
  if (!RESCUE.hw) {
    box.innerHTML = loading("Reading CPU, GPU, sensors and memory...");
    invoke("hardware_overview").then((hw) => { RESCUE.hw = hw; if (current === "rescue" && RESCUE.tab === "hardware") drawRsHardware(); }).catch((e) => { box.innerHTML = `<div class="note warn-n">${esc(String(e))}</div>`; });
    return;
  }
  const hw = RESCUE.hw;
  const c = hw.cpu || {};
  const gpus = arr(hw.gpu);
  const temps = arr(hw.sensors?.temps);
  const fans = arr(hw.sensors?.fans);
  const sticks = arr(hw.memory?.sticks);
  const mb = hw.motherboard || {};
  const gpuCards = gpus.map((g) => `<div class="card"><div class="spread"><div><div class="label">GPU</div><b>${esc(g.name || "Unknown GPU")}</b></div>${g.temp_c ? `<span class="pill ${+g.temp_c > 80 ? "high" : +g.temp_c > 65 ? "medium" : "low"}">${esc(g.temp_c)} °C</span>` : ""}</div>
    <div class="grid g4" style="gap:8px;margin-top:10px">
      ${kv("VRAM", g.vram_mb ? `${g.vram_mb} MB` : g.vram_bytes ? bytes(g.vram_bytes) : null)}
      ${kv("VRAM used", g.vram_used_mb ? `${g.vram_used_mb} MB` : g.vram_used_bytes ? bytes(g.vram_used_bytes) : null)}
      ${kv("Core clock", g.clock_mhz ? `${g.clock_mhz} MHz` : null)}
      ${kv("Utilization", g.util_percent != null ? `${g.util_percent} %` : null)}
      ${kv("Power", g.power_w ? `${g.power_w} W` : null)}
      ${kv("Driver", g.driver || null)}
      ${kv("Resolution", g.resolution && g.resolution !== "0x0@0Hz" ? g.resolution : null)}
      ${kv("Source", g.source || null)}
    </div></div>`).join("");
  box.innerHTML = `<div class="row"><button class="btn small" id="hw-re">⟳ Re-read sensors</button><span class="muted" style="align-self:center">Live from WMI / sysfs / nvidia-smi / dmidecode - nothing is faked; missing values mean the platform doesn't expose them.</span></div>
  <div class="grid g2" style="margin-top:12px">
    <div class="card"><div class="label">CPU</div><b>${esc(c.name || "Unknown CPU")}</b>
      <div class="grid g3" style="gap:8px;margin-top:10px">
        ${kv("Cores", c.cores)}${kv("Threads", c.threads)}${kv("Speed", c.current_mhz ? `${c.current_mhz} MHz` : null)}
        ${kv("Max", c.max_mhz ? `${c.max_mhz} MHz` : null)}${kv("Load", c.load_percent != null ? `${c.load_percent} %` : null)}${kv("Socket", c.socket || null)}
        ${kv("Vendor", c.vendor || null)}
      </div></div>
    <div class="card"><div class="label">Motherboard / BIOS</div>
      <div class="grid g2" style="gap:8px;margin-top:10px">
        ${kv("Board", [mb.manufacturer, mb.product].filter(Boolean).join(" ") || null)}
        ${kv("BIOS", [mb.bios_vendor, mb.bios_version].filter(Boolean).join(" ") || null)}
      </div>
      <div class="label" style="margin-top:14px">Memory</div>
      ${sticks.length ? `<table class="table" style="margin-top:6px"><tbody>${sticks.map((s) => `<tr><td class="mono muted">${esc(s.slot || "?")}</td><td>${s.size_bytes ? bytes(s.size_bytes) : "?"}</td><td class="muted">${esc(s.speed_mts || s.configured_mts ? `${s.speed_mts || s.configured_mts} MT/s` : "")}</td><td class="muted">${esc(s.vendor || "")} ${esc(s.part || "")}</td></tr>`).join("")}</tbody></table>` : '<div class="muted" style="margin-top:6px">Stick details unavailable on this platform (needs admin/root for DMI).</div>'}
    </div>
  </div>
  <div class="grid g2" style="margin-top:12px">
    ${gpuCards || '<div class="card"><div class="label">GPU</div><div class="muted">No GPU enumerated on this machine.</div></div>'}
    <div class="card"><div class="label">Sensors</div>
      ${temps.length ? temps.map((t) => `<div class="spread" style="padding:4px 0"><span class="mono muted">${esc(t.name)}</span><b class="${t.temp_c > 80 ? "danger-t" : ""}">${t.temp_c} °C</b></div>`).join("") : '<div class="muted">No temperature sensors exposed here.</div>'}
      ${fans.length ? fans.map((f) => `<div class="spread" style="padding:4px 0"><span class="mono muted">${esc(f.name)}</span><b>${num(f.rpm)} RPM</b></div>`).join("") : ""}
      ${hw.sensors?.note ? `<div class="cell-sub" style="margin-top:8px">${esc(hw.sensors.note)}</div>` : ""}
    </div>
  </div>`;
  $("#hw-re").onclick = () => { RESCUE.hw = null; drawRsHardware(); };
  const kvfn = box.querySelectorAll("[data-kv]");
  void kvfn;
}
function kv(label, value) {
  return `<div><div class="label">${esc(label)}</div><div class="v">${value == null ? '<span class="muted">—</span>' : esc(String(value))}</div></div>`;
}

// ------------------------------------------------------ integrity tab ----
function drawRsIntegrity() {
  const box = $("#rs-body");
  const st = RESCUE.integrity || {};
  box.innerHTML = `<div class="card">
    <div class="spread"><div><div class="label">FMOD replacement · system file integrity</div>
      <h3>${st.available ? `Baseline: ${num(st.files)} files · ${new Date(st.created * 1000).toLocaleString()}` : "No baseline yet"}</h3>
      <div class="muted" style="margin-top:4px">Snapshots hashes of every file in the system directories, then later scans show what changed, vanished or appeared. Works on Windows, Linux and macOS.</div>
      ${st.available && st.roots ? `<div class="muted mono" style="margin-top:6px;font-size:11.5px">Watching: ${arr(st.roots).map(esc).join(" · ")}</div>` : ""}
    </div></div>
    <div class="row" style="margin-top:14px">
      <button class="btn primary" id="ig-base">${st.available ? "⟳ Re-baseline" : "Create baseline"}</button>
      <button class="btn" id="ig-verify" ${st.available ? "" : "disabled"}>Verify against baseline</button>
    </div>
    <div id="ig-out" style="margin-top:12px"></div></div>`;
  $("#ig-base").onclick = async () => {
    if (st.available && !(await choose("Replace baseline?", "<p class=\"muted\">The current baseline will be overwritten with a fresh snapshot. Do this after intentional changes (updates, new software).</p>", [["go", "Re-baseline", "primary"]]))) return;
    const id = await guard(() => invoke("integrity_baseline", {}));
    if (id) { $("#ig-out").innerHTML = '<div class="muted"><span class="spin sm"></span> Snapshotting system files... this can take several minutes.</div>'; watchJob(id, async () => { RESCUE.integrity = await invoke("integrity_status").catch(() => null); drawRsIntegrity(); toast("Integrity baseline created."); }); }
  };
  $("#ig-verify").onclick = async () => {
    const id = await guard(() => invoke("integrity_verify"));
    if (id) { $("#ig-out").innerHTML = '<div class="muted"><span class="spin sm"></span> Hashing and comparing...</div>'; watchJob(id, () => showIntegrityResult(id)); }
  };
}
async function showIntegrityResult(jobId) {
  const jobs = await invoke("jobs_list").catch(() => []);
  const job = arr(jobs).find((j) => j.id === jobId);
  if (!job) return;
  const lines = arr(job.output).map((l) => { try { return JSON.parse(l); } catch { return null; } }).filter(Boolean);
  const summary = lines.find((l) => l.state === "summary") || {};
  const diffs = lines.filter((l) => l.state !== "summary");
  const out = $("#ig-out");
  const clean = !diffs.length;
  out.innerHTML = `<div class="note ${clean ? "" : "warn-n"}">${clean ? `<b>✔ All ${num(summary.scanned || 0)} files match the baseline.</b>` : `<b>${num(diffs.length)} difference(s) found</b> · ${summary.changed || 0} changed · ${summary.vanished || 0} vanished · ${summary.added || 0} added (of ${num(summary.scanned || 0)} scanned)`}</div>
    ${!clean ? `<div class="scroll" style="max-height:45vh;margin-top:8px"><table class="table"><thead><tr><th>State</th><th>File</th></tr></thead><tbody>
      ${diffs.slice(0, 500).map((d) => `<tr><td><span class="pill ${d.state === "vanished" ? "high" : d.state === "changed" ? "medium" : ""}">${esc(d.state)}</span></td><td class="mono" style="font-size:11.5px">${esc(d.path)}</td></tr>`).join("")}</tbody></table></div>` : ""}`;
}

// -------------------------------------------------------- catalog tab ----
function drawRsCatalog() {
  const box = $("#rs-body");
  const tools = arr(RESCUE.catalog?.tools);
  const cats = [...new Set(tools.map((t) => t.category))];
  box.innerHTML = `<input class="search cc-search" id="rs-q" placeholder="Find a tool, or the MRI tool it replaces...">
    <div id="rs-list">${cats.map((cat) => catBlock(cat, tools.filter((t) => t.category === cat))).join("")}</div>`;
  const input = $("#rs-q");
  input.oninput = () => {
    const q = input.value.toLowerCase().trim();
    $("#rs-list").innerHTML = q
      ? catBlock("Results", tools.filter((t) => JSON.stringify(t).toLowerCase().includes(q)))
      : cats.map((cat) => catBlock(cat, tools.filter((t) => t.category === cat))).join("");
    wireCatalog();
  };
  wireCatalog();
}
function catBlock(cat, tools) {
  if (!tools.length) return "";
  const kindPill = (k) => k === "builtin" ? '<span class="pill low">built into MWM</span>' : k === "bundled" ? '<span class="pill">bundled in ISO</span>' : '<span class="pill medium">fetched on demand</span>';
  return `<div class="cc-node-heading"><h3>${esc(cat)}</h3><span class="muted">${tools.length}</span></div>
  <div class="cc-cards">${tools.map((t) => `<div class="card cc-app"><div class="spread"><strong>${esc(t.name)}</strong>${kindPill(t.kind)}</div>
    <div class="muted" style="font-size:12px;margin:4px 0">Replaces: ${esc(t.replaces)} · ${esc(t.license)}</div>
    <div class="muted" style="font-size:12.5px">${esc(t.notes)}</div>
    <div class="row" style="margin-top:10px">
      ${t.kind === "builtin" ? `<button class="btn small primary" data-rs-go="${esc(t.target)}">Open in MWM</button>` : ""}
      ${t.kind === "download" ? `<button class="btn small" data-rs-dl="${esc(t.id)}" ${t.installed ? "disabled" : ""}>${t.installed ? "✔ Downloaded" : "Get from vendor"}</button>` : ""}
      ${t.kind === "bundled" ? `<a class="btn small" href="${esc(t.target)}" target="_blank" rel="noopener noreferrer">Project ↗</a>` : ""}
      ${t.kind === "download" && t.installed ? `<button class="btn small" data-rs-loc="${esc(t.id)}">Open folder</button>` : ""}
    </div></div>`).join("")}</div>`;
}
function wireCatalog() {
  $$("[data-rs-go]").forEach((b) => (b.onclick = () => {
    const target = b.dataset.rsGo;
    if (target === "rescue-integrity") { RESCUE.tab = "integrity"; drawRescue(); }
    else go(target);
  }));
  $$("[data-rs-dl]").forEach((b) => (b.onclick = async () => {
    b.disabled = true; b.textContent = "Downloading...";
    const r = await guard(() => invoke("rescue_fetch", { id: b.dataset.rsDl }));
    if (r) { toast(`Saved to MWM rescue folder${r.saved?.[0]?.vendor_page ? " (vendor page + direct links)" : ""}.`); RESCUE.catalog = null; const c = await invoke("rescue_catalog").catch(() => null); RESCUE.catalog = c; drawRsCatalog(); }
    else { b.disabled = false; b.textContent = "Get from vendor"; }
  }));
  $$("[data-rs-loc]").forEach((b) => (b.onclick = async () => {
    const r = await guard(() => invoke("rescue_locate", { id: b.dataset.rsLoc }));
    if (r) {
      const files = arr(r.files);
      modal(`<h2>${esc(b.dataset.rsLoc)} downloads</h2><div class="mono muted" style="font-size:12px">${esc(r.dir)}</div>
        <div class="scroll" style="max-height:50vh;margin-top:10px">${files.map((f) => `<div class="spread" style="padding:6px 0;border-bottom:1px solid var(--border)"><span class="mono" style="font-size:12px">${esc(f.name)}</span><span class="muted">${bytes(f.size)}</span></div>`).join("") || "empty"}</div>
        <div class="row" style="justify-content:flex-end;margin-top:14px"><button class="btn" data-close>Close</button>${DEMO || TARGET ? "" : `<button class="btn primary" id="rs-open-dir">Open folder</button>`}</div>`);
      const od = $("#rs-open-dir");
      if (od) od.onclick = () => localInvoke("reveal", { path: r.dir });
    }
  }));
}
