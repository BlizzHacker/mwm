/* Malware Lab: local static triage with optional Arkana deep analysis. */
"use strict";

const LAB = { path: "", result: null, status: null, quarantine: [], arkanaSource: "", arkanaHost: "" };

const arkanaInvoke = (cmd, args = {}) => LAB.arkanaSource === TARGET
  ? invoke(cmd, args)
  : localInvoke("remote_call", { id: LAB.arkanaSource, cmd, args });

async function findArkana() {
  const selected = await invoke("arkana_status").catch(() => null);
  if (selected?.reachable) return { status: selected, source: TARGET, host: "Selected machine" };
  const connections = arr(await localInvoke("conn_list").catch(() => []));
  const results = await Promise.allSettled(connections.filter((c) => c.id !== TARGET).map(async (c) => ({
    source: c.id, host: c.name,
    status: await localInvoke("remote_call", { id: c.id, cmd: "arkana_status", args: {} }),
  })));
  const connected = results.find((r) => r.status === "fulfilled" && r.value.status?.reachable);
  return connected?.value || { status: selected || {}, source: TARGET, host: "Selected machine" };
}

function openLab(path = "") {
  LAB.path = path;
  LAB.result = null;
  go("lab");
  if (path) labAnalyze();
}

VIEWS.lab = async function () {
  $("#top-actions").innerHTML = '<button class="btn" id="lab-refresh">Refresh Arkana</button>';
  $("#lab-refresh").onclick = () => { LAB.status = null; VIEWS.lab(); };
  $("#page").innerHTML = `
    <div class="card">
      <h3 style="margin:0 0 8px">Analyze a file</h3>
      <p class="muted">Static triage runs on the selected machine. It reports hashes, structure, and suspicious indicators. Family matching is available on Linux; Windows uses Defender or a connected Arkana server for malware verdicts.</p>
      <div class="row" style="flex-wrap:wrap"><input class="search mono" id="lab-path" style="flex:1;min-width:260px" placeholder="Full path on selected machine" value="${esc(LAB.path)}">
        <button class="btn" id="lab-pick">Browse</button><button class="btn primary" id="lab-analyze">Analyze</button></div>
      <div id="lab-result" style="margin-top:14px"></div>
    </div>
    <div class="grid g2" style="margin-top:14px">
      <div class="card" id="lab-arkana">${loading("Checking Arkana...")}</div>
      <div class="card" id="lab-quarantine">${loading("Reading quarantine...")}</div>
    </div>`;
  $("#lab-pick").onclick = async () => { const paths = await pickFiles(); if (paths.length) { LAB.path = paths[0]; $("#lab-path").value = LAB.path; } };
  $("#lab-analyze").onclick = () => { LAB.path = $("#lab-path").value.trim(); labAnalyze(); };
  $("#lab-path").onkeydown = (e) => { if (e.key === "Enter") $("#lab-analyze").click(); };
  if (LAB.result) drawLabResult();
  const [arkana, quarantine] = await Promise.all([
    guard(findArkana), guard(() => invoke("lab_quarantine_list")),
  ]);
  if (current !== "lab") return;
  LAB.status = arkana?.status || null;
  LAB.arkanaSource = arkana?.source ?? TARGET;
  LAB.arkanaHost = arkana?.host || "Selected machine";
  LAB.quarantine = arr(quarantine);
  drawLabArkana();
  drawLabQuarantine();
};

async function labAnalyze() {
  if (!LAB.path) return toast("Choose a file.", true);
  const box = $("#lab-result");
  if (box) box.innerHTML = loading("Analyzing file locally...");
  LAB.result = await guard(() => invoke("lab_analyze", { path: LAB.path })) || null;
  if (current === "lab") drawLabResult();
}

function drawLabResult() {
  const box = $("#lab-result");
  if (!box) return;
  const a = LAB.result;
  if (!a) { box.innerHTML = ""; return; }
  const risk = a.score >= 60 ? "high" : a.score >= 30 ? "medium" : "low";
  box.innerHTML = `<div class="grid g4">
    <div class="card"><div class="label">Verdict</div><div class="v"><span class="pill ${risk}">${esc(a.verdict)}</span></div><div class="muted">Score ${a.score}/100</div></div>
    <div class="card"><div class="label">Type</div><div class="v">${esc(a.format || "unknown")}</div><div class="muted">${esc(a.arch || "")} ${esc(a.kind || "")}</div></div>
    <div class="card"><div class="label">Size</div><div class="v">${bytes(a.size)}</div><div class="muted">Entropy ${Number(a.entropy || 0).toFixed(2)}</div></div>
    <div class="card"><div class="label">Family matches</div><div class="v">${arr(a.families).length}</div><div class="muted">Indicator matches need review</div></div>
    </div>
    <div class="note" style="margin-top:10px"><b>${esc(a.name)}</b><div class="mono wrap">SHA-256 ${esc(a.sha256)}</div>${arr(a.reasons).length ? `<div>${arr(a.reasons).map(esc).join(" · ")}</div>` : ""}</div>
    <div class="row" style="margin:12px 0;flex-wrap:wrap"><button class="btn" id="lab-av">Antivirus second opinion</button><button class="btn" id="lab-deep" ${LAB.status?.reachable && LAB.arkanaSource === TARGET ? "" : "disabled"}>Analyze with Arkana</button><button class="btn danger" id="lab-q">Quarantine file</button></div>
    ${LAB.status?.reachable && LAB.arkanaSource !== TARGET ? '<p class="muted">To send this file to Arkana, select its Proxmox host and choose a file there. Cross-machine sample transfer is not available yet.</p>' : ""}
    <div id="lab-extra"></div>
    <div class="grid g2"><div class="card"><h3>Family matches</h3>${arr(a.families).length ? arr(a.families).map((f) => `<div class="note" style="margin:8px 0"><b>${esc(f.family)}</b> · ${f.confidence}%<div class="muted">${esc(arr(f.evidence).join("; "))}</div></div>`).join("") : '<div class="muted">No family match reported. For a malware verdict, use your antivirus or Arkana.</div>'}</div>
    <div class="card"><h3>Capabilities and indicators</h3>${arr(a.capabilities).slice(0, 25).map((c) => `<div><b>${esc(c.category)}</b> · ${esc(c.risk)} <span class="muted">${esc(c.meaning)}</span></div>`).join("") || '<div class="muted">No risky imports found.</div>'}
    ${arr(a.iocs).length ? `<div class="sep"></div>${arr(a.iocs).slice(0, 40).map((i) => `<div class="mono wrap">${esc(i.kind)} ${esc(i.value)}</div>`).join("")}` : ""}</div></div>`;
  $("#lab-av").onclick = async () => { $("#lab-extra").textContent = "Scanning..."; const r = await guard(() => invoke("lab_av_scan", { path: LAB.path })); $("#lab-extra").textContent = r || ""; };
  $("#lab-deep").onclick = async () => { const id = await guard(() => arkanaInvoke("arkana_analyze", { path: LAB.path })); if (id) watchJob(id); };
  $("#lab-q").onclick = async () => {
    if (!(await choose("Quarantine this file?", `<p class="muted mono wrap">${esc(LAB.path)}</p><p>The original file will be removed and stored in MWM quarantine. You can restore it later.</p>`, [["yes", "Quarantine", "danger"]]))) return;
    const item = await guard(() => invoke("lab_quarantine", { path: LAB.path, reason: LAB.result.verdict }));
    if (item) { toast("File quarantined."); LAB.result = null; VIEWS.lab(); }
  };
}

function drawLabArkana() {
  const box = $("#lab-arkana"); if (!box) return;
  const s = LAB.status || {};
  box.innerHTML = `<h3 style="margin:0 0 8px">Arkana</h3><p class="muted">Deep analysis uses your Arkana server. The API key stays on the selected machine and is never displayed here.</p>
    <div style="margin-bottom:12px"><span class="pill ${s.reachable ? "low" : "medium"}">${s.reachable ? "Connected" : s.configured ? "Unreachable" : "Not configured"}</span> <span class="muted">via ${esc(LAB.arkanaHost)}</span> <span class="mono muted">${esc(s.url || "")}</span>${s.lxc_vmid ? ` <span class="muted">· LXC ${esc(s.lxc_vmid)}</span>` : ""}</div>
    <div class="row" style="flex-wrap:wrap"><input class="search mono" id="ark-url" style="flex:1;min-width:220px" placeholder="Arkana address, e.g. http://host:8082" value="${esc(s.url || "")}"><input class="search mono" type="password" id="ark-key" placeholder="API key" autocomplete="off"></div>
    <div class="row" style="margin-top:8px;flex-wrap:wrap"><input class="search mono" id="ark-samples" style="flex:1;min-width:220px" placeholder="Local path mounted as Arkana /samples" value="${esc(s.samples_dir || "")}"><button class="btn" id="ark-connect">Connect</button></div>
    <div class="row" style="margin-top:12px;flex-wrap:wrap">${s.pve ? '<button class="btn primary" id="ark-lxc">Connect Arkana LXC</button>' : ""}${s.managed ? '<button class="btn" id="ark-start">Start</button><button class="btn" id="ark-stop">Stop</button><button class="btn" id="ark-update">Update</button>' : `<button class="btn" id="ark-install" ${s.docker && s.compose && s.git ? "" : "disabled"}>Install Arkana here</button>`}</div>
    ${!s.docker || !s.compose || !s.git ? '<div class="muted" style="margin-top:8px">Self-install requires Docker Compose and Git on the selected machine.</div>' : ""}`;
  $("#ark-connect").onclick = async () => {
    const r = await guard(() => arkanaInvoke("arkana_configure", { url: $("#ark-url").value.trim(), key: $("#ark-key").value, samplesDir: $("#ark-samples").value.trim() }));
    if (r) { LAB.status = r; toast("Arkana connected."); drawLabArkana(); drawLabResult(); }
  };
  $("#ark-lxc") && ($("#ark-lxc").onclick = async () => {
    const vmid = await ask("Connect Arkana in an LXC", "LXC ID on this Proxmox node", s.lxc_vmid || "", "Connect");
    if (!vmid) return;
    const result = await guard(() => arkanaInvoke("arkana_connect_lxc", { vmid: vmid.trim() }));
    if (result) { LAB.status = result; toast(`Arkana LXC ${vmid} connected.`); drawLabArkana(); drawLabResult(); }
  });
  $("#ark-install") && ($("#ark-install").onclick = async () => { const id = await guard(() => arkanaInvoke("arkana_install", {})); if (id) watchJob(id, () => { LAB.status = null; if (current === "lab") VIEWS.lab(); }); });
  for (const action of ["start", "stop", "update"]) {
    const b = $(`#ark-${action}`); if (!b) continue;
    b.onclick = async () => { const r = await guard(() => arkanaInvoke("arkana_control", { action })); if (r?.job) watchJob(r.job, () => { if (current === "lab") VIEWS.lab(); }); else if (r) { toast(r.ok ? `Arkana ${action} complete.` : r.output, !r.ok); VIEWS.lab(); } };
  }
}

function drawLabQuarantine() {
  const box = $("#lab-quarantine"); if (!box) return;
  box.innerHTML = `<h3 style="margin:0 0 8px">Quarantine</h3>${LAB.quarantine.length ? `<div class="scroll"><table class="table"><thead><tr><th>File</th><th>Reason</th><th>Actions</th></tr></thead><tbody>${LAB.quarantine.map((q, i) => `<tr><td class="mono wrap">${esc(q.original)}</td><td>${esc(q.reason)}</td><td><button class="btn small" data-q-restore="${i}">Restore</button> <button class="btn small danger" data-q-delete="${i}">Delete</button></td></tr>`).join("")}</tbody></table></div>` : '<div class="muted">No files quarantined.</div>'}`;
  $$("[data-q-restore]", box).forEach((b) => b.onclick = async () => { const q = LAB.quarantine[+b.dataset.qRestore]; if (await guard(() => invoke("lab_quarantine_restore", { id: q.id }))) { toast("File restored."); VIEWS.lab(); } });
  $$("[data-q-delete]", box).forEach((b) => b.onclick = async () => { const q = LAB.quarantine[+b.dataset.qDelete]; if (!(await choose("Delete quarantined file?", `<p class="mono wrap">${esc(q.original)}</p>`, [["yes", "Delete permanently", "danger"]]))) return; await guard(() => invoke("lab_quarantine_delete", { id: q.id })); VIEWS.lab(); });
}
