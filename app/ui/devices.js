/* Dense live snapshot for every managed MWM host and known network device. */
"use strict";

const DEVICE_VIEW = { connections: [], local: null, localSrv: null, pve: null, network: null, map: [], remotes: [], updated: null };
const deviceRows = (v) => Array.isArray(v) ? v : Array.isArray(v?.devices) ? v.devices : [];
const deviceHost = (v) => { try { return new URL(v).hostname; } catch { return ""; } };
const deviceTry = async (fn) => { try { return await fn(); } catch { return null; } };
const deviceSame = (a, b) => !!a && !!b && String(a).trim().toLowerCase() === String(b).trim().toLowerCase();
function dNum(v) { if (v === null || v === undefined || v === "") return null; const n = Number(v); return Number.isFinite(n) ? n : null; }
function dBytes(v) { const n = dNum(v); return n === null ? "Unavailable" : bytes(n); }
function dPct(used, total) { const u = dNum(used), t = dNum(total); return u === null || !t ? null : Math.max(0, Math.min(100, u * 100 / t)); }
function dDuration(v) {
  const n = dNum(v); if (n === null) return "Unavailable";
  let s = Math.max(0, Math.floor(n)); const d = Math.floor(s / 86400); s %= 86400;
  const h = Math.floor(s / 3600); s %= 3600; const m = Math.floor(s / 60);
  return (d ? d + "d " : "") + (h ? h + "h " : "") + m + "m";
}
function dState(v, detail) {
  if (v === true || v === "online" || v === "running") return ["ONLINE", "low"];
  if (v === false || v === "offline" || v === "stopped") return ["OFFLINE", "high"];
  if (v === "planned") return ["PLANNED", ""];
  if (v === "leased") return ["LEASE", "medium"];
  if (v === "last_seen" || (v === "unknown" && /Last observed|Last seen/i.test(detail || ""))) return ["LAST SEEN", "medium"];
  return ["UNKNOWN", "medium"];
}
function dMetric(label, value, sub, title) {
  return '<div class="device-stat" title="' + esc(title || "") + '"><div class="device-stat-label">' + esc(label) + '</div><div class="device-stat-value">' + esc(value || "Unavailable") + '</div><div class="device-stat-sub">' + esc(sub || "") + '</div></div>';
}
function dFact(label, value) {
  return '<div class="device-fact"><b>' + esc(label) + '</b><span>' + esc(value || "Unavailable") + '</span></div>';
}
function dLoad(srv, cores) {
  const a = String(srv?.load || "").trim().split(/\s+/).slice(0, 3).map(Number).filter(Number.isFinite);
  if (!a.length) return ["Unavailable", "Load average not reported"];
  const per = cores ? " · " + (a[0] / cores).toFixed(2) + "/core" : "";
  return ["1m " + a[0].toFixed(2) + per, "5m " + (a[1] ?? a[0]).toFixed(2) + " · 15m " + (a[2] ?? a[0]).toFixed(2)];
}
function dVolumes(info, srv) {
  const sysDisks = Array.isArray(info?.disks) ? info.disks : [];
  const mounts = Array.isArray(srv?.mounts) && srv.mounts.length ? srv.mounts : sysDisks;
  return mounts.filter((x) => dNum(x.total) > 0).sort((a, b) => (dNum(b.total) || 0) - (dNum(a.total) || 0));
}
function dPrimaryDisk(info, srv, pveNode) {
  if (pveNode && dNum(pveNode.maxdisk) > 0) return { total: pveNode.maxdisk, used: pveNode.disk, free: Math.max(0, Number(pveNode.maxdisk) - Number(pveNode.disk)), mount: "Proxmox node root" };
  const v = dVolumes(info, srv);
  if (!v.length) return null;
  const x = v.find((x) => x.mount === "/" || x.mount === "C:\\" || x.mount === "C:/") || v[0];
  return { total: x.total, used: x.used ?? (Number(x.total) - Number(x.free)), free: x.free ?? (Number(x.total) - Number(x.used || 0)), mount: x.mount || x.name || "Volume" };
}
function dGpuRows(srv) { return Array.isArray(srv?.gpus) ? srv.gpus : []; }
function dHostCard(m) {
  const i = m.info || {}, s = m.srv || {}, n = m.pveNode || null;
  const cores = dNum(n?.maxcpu) || dNum(i.cores) || 0;
  const cpuPct = n ? (dNum(n.cpu) === null ? null : dNum(n.cpu) * 100) : dNum(s.cpu_usage_pct);
  const ramTotal = dNum(n?.maxmem) || dNum(i.memory_total) || 0;
  const ramUsed = dNum(n?.mem) ?? dNum(i.memory_used);
  const ramPct = dPct(ramUsed, ramTotal);
  const disk = dPrimaryDisk(i, s, n);
  const diskPct = disk ? dPct(disk.used, disk.total) : null;
  const gpus = dGpuRows(s);
  const vramTotal = gpus.reduce((a, g) => a + (dNum(g.vram_total_bytes) || 0), 0);
  const vramUsed = gpus.reduce((a, g) => a + (dNum(g.vram_used_bytes) || 0), 0);
  const gpuLoad = gpus.map((g) => dNum(g.utilization_pct)).filter((x) => x !== null);
  const gpuTemp = gpus.map((g) => dNum(g.temp_c)).filter((x) => x !== null);
  const load = dLoad(s, cores);
  const drives = Array.isArray(s.drives) && s.drives.length ? s.drives : Array.isArray(s.smart) && s.smart.length ? s.smart : Array.isArray(s.unraid?.disks) ? s.unraid.disks : [];
  const volumes = dVolumes(i, s);
  const issues = [];
  if (arr(s.failed).length) issues.push(arr(s.failed).length + " failed services");
  if (arr(s.zfs).some((x) => x.health && x.health !== "ONLINE")) issues.push("degraded ZFS");
  if (drives.some((x) => x.passed === false)) issues.push("SMART failure");
  if (diskPct !== null && diskPct >= 90) issues.push("disk " + Math.round(diskPct) + "% used");
  const isUp = m.online !== undefined ? !!m.online : (n?.status === "online");
  const [label, tone] = dState(isUp);
  const alert = issues.length > 0;
  const status = !isUp ? ["OFFLINE", "high"] : alert ? ["ATTENTION", "medium"] : [label, tone];
  const addr = m.address ? '<span class="mono device-address">' + esc(m.address) + '</span>' : "";
  const os = i.os || ({ linux: "Linux", windows: "Windows", macos: "macOS" }[i.platform] || i.platform || "Host");
  const title = m.name || i.hostname || n?.node || "MWM host";
  const hostSub = [i.hostname, os, m.latency ? m.latency + " ms" : ""].filter(Boolean).join(" · ");
  const coreText = cores ? cores + " logical cores" : "Core count unavailable";
  const cpuValue = cpuPct === null ? "Unavailable" : Math.round(cpuPct) + "%";
  const cpuSub = coreText + (n ? " · Proxmox node" : " · sampled by MWM");
  const ramValue = ramPct === null ? "Unavailable" : Math.round(ramPct) + "%";
  const ramSub = ramTotal ? dBytes(ramUsed) + " / " + dBytes(ramTotal) : "Memory totals unavailable";
  const vramValue = gpus.length ? (vramTotal ? dBytes(vramUsed) + " / " + dBytes(vramTotal) : "GPU found") : "No GPU reported";
  const vramSub = gpus.length ? gpus.map((g) => g.name || "GPU").join(", ") + (gpuLoad.length ? " · " + Math.round(gpuLoad.reduce((a,b)=>a+b,0)/gpuLoad.length) + "% GPU" : "") + (gpuTemp.length ? " · " + Math.round(gpuTemp.reduce((a,b)=>a+b,0)/gpuTemp.length) + "°C" : "") : "Telemetry missing or no GPU";
  const diskValue = disk ? dBytes(disk.free) + " free" : "Unavailable";
  const diskSub = disk ? dBytes(disk.used) + " used / " + dBytes(disk.total) + " · " + (diskPct === null ? "usage n/a" : Math.round(diskPct) + "% used") : "No disk totals reported";
  const uptime = n?.uptime ?? i.uptime_secs;
  const guestCount = n && DEVICE_VIEW.pve?.guests ? DEVICE_VIEW.pve.guests.filter((g) => g.node === n.node) : [];
  const guestUp = guestCount.filter((g) => g.status === "running").length;
  const metrics = [
    dMetric("CPU / CORES", cpuValue, cpuSub, i.cpu || "CPU model not reported"),
    dMetric("LOAD", load[0], load[1], "Load average is runnable work, shown separately from CPU utilization."),
    dMetric("RAM", ramValue, ramSub),
    dMetric("GPU / VRAM", vramValue, vramSub),
    dMetric("DISK SPACE", diskValue, diskSub, disk?.mount || ""),
    dMetric("UPTIME", dDuration(uptime), n ? guestUp + "/" + guestCount.length + " guests on this node" : "MWM host uptime")
  ].join("");
  const driveFacts = drives.length ? drives.map((d) => {
    const cap = dBytes(d.capacity || d.total);
    const health = d.passed === false ? "SMART FAIL" : d.passed === true ? "SMART OK" : (d.status || "health unavailable");
    const temp = d.temp !== undefined && d.temp !== null ? " · " + d.temp + "°C" : "";
    return (d.model || d.name || d.device || "Drive") + " · " + cap + " · " + health + temp;
  }).join(" | ") : "Physical drive / bay telemetry not reported";
  const gpuFacts = gpus.length ? gpus.map((g) => (g.name || "GPU") + " · " + (dNum(g.vram_used_bytes) !== null ? dBytes(g.vram_used_bytes) + " / " + dBytes(g.vram_total_bytes) : "VRAM usage unavailable") + (dNum(g.utilization_pct) !== null ? " · " + Math.round(Number(g.utilization_pct)) + "% load" : "") + (dNum(g.temp_c) !== null ? " · " + Math.round(Number(g.temp_c)) + "°C" : "")).join(" | ") : "No GPU telemetry reported";
  const volumeMarkup = volumes.length ? '<details class="device-volumes"><summary>All volumes (' + volumes.length + ')</summary><div class="device-volume-list">' + volumes.map((v) => {
    const free = dNum(v.free) ?? Math.max(0, (dNum(v.total) || 0) - (dNum(v.used) || 0));
    const used = dNum(v.used) ?? Math.max(0, (dNum(v.total) || 0) - free);
    const pct = dPct(used, v.total);
    return '<div><b>' + esc(v.mount || v.name || "Volume") + '</b><span>' + esc(dBytes(free) + " free / " + dBytes(v.total) + (pct === null ? "" : " · " + Math.round(pct) + "% used")) + '</span></div>';
  }).join("") + '</div></details>' : "";
  const facts = '<div class="device-facts">' +
    dFact("Processor", i.cpu || "CPU model unavailable") +
    dFact("Graphics / VRAM", gpuFacts) +
    dFact("Physical drives / bays", drives.length ? drives.length + " reported · " + driveFacts : driveFacts) +
    (n ? dFact("Node guests", guestUp + " running / " + guestCount.length + " total") : "") +
    (issues.length ? dFact("Attention", issues.join(" · ")) : "") +
    volumeMarkup + '</div>';
  const manage = m.connection ? '<button class="btn small" data-device-target="' + esc(m.connection.id) + '">Manage</button>' : '<button class="btn small" data-device-go="fleet">Manage</button>';
  return '<article class="card device-snapshot-card ' + (!isUp ? "is-offline" : "") + '">' +
    '<div class="device-snapshot-head"><span class="device-glyph">' + icon(m.icon || "server") + '</span><div class="device-title-wrap"><strong>' + esc(title) + '</strong><small>' + esc(hostSub) + '</small></div>' +
    '<span class="pill ' + status[1] + '">' + status[0] + '</span></div>' +
    '<div class="device-snapshot-address">' + addr + (m.detail ? '<span class="muted">' + esc(m.detail) + '</span>' : "") + '</div>' +
    '<div class="device-stat-grid">' + metrics + '</div>' + facts +
    '<div class="row device-snapshot-actions">' + manage + '</div></article>';
}
function dInterface(fw, name, label) {
  const stats = fw?.interfaces?.statistics || {};
  const entries = Object.entries(stats).filter(([,v]) => v.name === name);
  const picked = entries.find(([,v]) => v.mtu !== undefined && v["received-bytes"] !== undefined) || entries.find(([,v]) => v["received-bytes"] !== undefined);
  if (!picked) return label + " " + name + ": counters unavailable";
  const x = picked[1];
  return label + " " + name + " · RX " + dBytes(x["received-bytes"]) + " · TX " + dBytes(x["sent-bytes"]) +
    " · errors " + ((Number(x["received-errors"]) || 0) + (Number(x["send-errors"]) || 0)) +
    " · drops " + (Number(x["dropped-packets"]) || 0);
}
function dApplianceCard(d) {
  const online = d.online === true;
  const [label, tone] = dState(d.online);
  const fw = d.id === "opnsense";
  if (!fw) {
    const fields = [
      dMetric("CPU / CORES", "Not exposed", "Router API does not report processor data"),
      dMetric("LOAD", "Not exposed", "No load average endpoint"),
      dMetric("RAM", "Not exposed", "Router API does not report memory"),
      dMetric("GPU / VRAM", "N/A", "Access point"),
      dMetric("DISK / BAYS", "Not exposed", "No disk inventory endpoint"),
      dMetric("UPTIME", "Not exposed", "No uptime endpoint")
    ].join("");
    const detail = [d.role, d.mode, d.http_status ? "HTTP " + d.http_status : "", d.latency_ms !== undefined ? d.latency_ms + " ms HTTP response" : "", d.firmware_update].filter(Boolean).join(" · ");
    return '<article class="card device-snapshot-card device-appliance-card"><div class="device-snapshot-head"><span class="device-glyph">' + icon("net") + '</span><div class="device-title-wrap"><strong>' + esc(d.name) + '</strong><small>' + esc(d.address || "") + '</small></div><span class="pill ' + tone + '">' + label + '</span></div><div class="device-snapshot-address"><span class="muted">' + esc(detail || d.error || "No device telemetry") + '</span></div><div class="device-stat-grid">' + fields + '</div><div class="device-facts">' + dFact("Device", d.role || "TP-Link network device") + dFact("Live check", online ? "Management page reachable" : d.error || "Management page unreachable") + dFact("Client count", "Not exposed by current integration") + '</div><div class="row device-snapshot-actions"><a class="btn small" href="' + esc(d.url || "http://192.168.0.2/") + '" target="_blank" rel="noopener noreferrer">Open router ↗</a></div></article>';
  }
  const memory = d.resources?.memory || {};
  const ramTotal = dNum(memory.total), ramUsed = dNum(memory.used);
  const ramPct = dPct(ramUsed, ramTotal);
  const rt = d.system_time || {};
  const root = (d.system_disk?.devices || []).find((x) => x.mountpoint === "/");
  const gw = (d.gateways?.items || []).find((x) => x.name === "WAN_DHCP") || (d.gateways?.items || [])[0];
  const services = d.services?.rows || [];
  const running = services.filter((x) => Number(x.running) === 1).length;
  const versions = (d.system?.versions || []).join(" · ");
  const fields = [
    dMetric("CPU / CORES", "Not exposed", "Firewall API does not return model or core count"),
    dMetric("LOAD", rt.loadavg || "Unavailable", "System load average"),
    dMetric("RAM", ramPct === null ? "Unavailable" : Math.round(ramPct) + "%", ramTotal ? dBytes(ramUsed) + " / " + dBytes(ramTotal) : "Memory unavailable"),
    dMetric("GPU / VRAM", "N/A", "Firewall appliance"),
    dMetric("DISK SPACE", root ? dBytes(root.available_bytes) + " free" : "Unavailable", root ? dBytes(root.used_bytes) + " used / " + dBytes(root.total_bytes) + " · " + (root.used_pct ?? "?") + "% used" : "Root filesystem not reported"),
    dMetric("UPTIME", rt.uptime || "Unavailable", rt.datetime || "System time unavailable")
  ].join("");
  const networkFacts = dFact("WAN", dInterface(d, d.wan_interface || "igb3", "WAN")) +
    dFact("LAN", dInterface(d, d.lan_interface || "igb0", "LAN"));
  const leasesError = d.leases?.error ? "DHCP API: " + d.leases.error + "; saved clients are last-seen only" : "Live DHCP leases: " + (deviceRows(d.leases).length || 0);
  return '<article class="card device-snapshot-card device-appliance-card"><div class="device-snapshot-head"><span class="device-glyph">' + icon("shield") + '</span><div class="device-title-wrap"><strong>' + esc(d.name) + '</strong><small>' + esc(d.address || "") + ' · ' + esc(versions) + '</small></div><span class="pill ' + tone + '">' + label + '</span></div><div class="device-snapshot-address"><span class="muted">Gateway ' + esc(gw?.status_translated || gw?.status || "unknown") + ' · ' + esc(gw?.address || "no WAN address") + ' · API ' + (d.api_connected ? "connected" : "unavailable") + '</span></div><div class="device-stat-grid">' + fields + '</div><div class="device-facts">' + dFact("Services", running + " running / " + (d.services?.rowCount ?? services.length) + " reported") + dFact("IDS / Suricata", d.ids?.status || "Not reported") + networkFacts + dFact("DHCP inventory", leasesError) + dFact("Firmware", d.firmware?.product?.CORE_VERSION || versions) + '</div><div class="row device-snapshot-actions"><a class="btn small" href="' + esc(d.url || "https://192.168.0.1/") + '" target="_blank" rel="noopener noreferrer">Open firewall ↗</a></div></article>';
}
function dInventoryCard(d) {
  const [label, tone] = dState(d.state, d.detail);
  const fields = [
    dMetric("CPU / CORES", "No MWM telemetry", "Host agent not connected"),
    dMetric("LOAD", "Unavailable", "No live host sample"),
    dMetric("RAM", "Unavailable", "No live host sample"),
    dMetric("GPU / VRAM", "Unavailable", "No live host sample"),
    dMetric("DISK / BAYS", "Unavailable", "No live host sample"),
    dMetric("UPTIME", "Unavailable", "No live host sample")
  ].join("");
  const action = d.href ? '<a class="btn small" href="' + esc(d.href) + '" target="_blank" rel="noopener noreferrer">Open management ↗</a>' : "";
  return '<article class="card device-snapshot-card device-inventory-card"><div class="device-snapshot-head"><span class="device-glyph">' + icon(d.icon || "net") + '</span><div class="device-title-wrap"><strong>' + esc(d.name || "LAN client") + '</strong><small>' + esc(d.role || "Network inventory") + '</small></div><span class="pill ' + tone + '">' + label + '</span></div><div class="device-snapshot-address">' + (d.address ? '<span class="mono">' + esc(d.address) + '</span>' : "") + '</div><div class="device-stat-grid">' + fields + '</div><div class="device-facts">' + dFact("Inventory", d.detail || "No live MWM connection") + '</div><div class="row device-snapshot-actions">' + action + '</div></article>';
}
function dSection(title, subtitle, cards) {
  return '<section class="device-snapshot-section"><div class="device-section-head"><div><h3>' + esc(title) + '</h3><div class="muted">' + esc(subtitle) + '</div></div><span class="pill">' + cards.length + '</span></div><div class="device-snapshot-grid">' + (cards.length ? cards.join("") : '<div class="empty">No devices reported.</div>') + '</div></section>';
}
function drawDeviceTopology() {
  if (current !== "topology") return;
  const remotes = DEVICE_VIEW.remotes;
  const clusterSource = remotes.find((r) => Array.isArray(r.srv?.nodes) && r.srv.nodes.length);
  const nodes = clusterSource?.srv.nodes || [];
  const guests = clusterSource?.srv.proxmox?.guests || [];
  DEVICE_VIEW.pve = clusterSource ? { nodes, guests } : null;
  const nodeModels = nodes.map((n) => {
    const c = remotes.find((x) => deviceSame(deviceHost(x.connection.url), n.ip) || deviceSame(x.info?.hostname, n.node));
    return { name: n.node, address: n.ip, pveNode: n, connection: c?.connection, online: n.status === "online", latency: c?.ms, info: c?.info || null, srv: c?.srv || null, detail: (c?.online ? "MWM connected" : c ? "PVE node up · MWM endpoint offline" : "PVE telemetry only") };
  });
  const pveAddresses = new Set(nodes.map((n) => String(n.ip || "").toLowerCase()).filter(Boolean));
  const nodeNames = new Set(nodes.map((n) => String(n.node || "").toLowerCase()));
  const otherHosts = remotes.filter((c) => !pveAddresses.has(deviceHost(c.connection.url).toLowerCase()) && !nodeNames.has(String(c.info?.hostname || "").toLowerCase())).map((c) => ({
    name: c.connection.name || c.info?.hostname || "MWM host", address: deviceHost(c.connection.url), connection: c.connection,
    info: c.info, srv: c.srv, online: c.online, latency: c.ms, detail: c.online ? "Managed MWM endpoint" : c.error || "Saved endpoint did not respond",
    icon: c.online ? "server" : "alert"
  }));
  const local = DEVICE_VIEW.local || {};
  const localModel = { name: local.hostname || "This PC", info: local, srv: DEVICE_VIEW.localSrv || {}, online: true, detail: "This MWM · local Windows/Linux/macOS host", icon: "server" };
  const primary = [];
  primary.push(dHostCard(localModel));
  nodeModels.forEach((m) => primary.push(dHostCard(m)));
  otherHosts.forEach((m) => primary.push(dHostCard(m)));
  const network = deviceRows(DEVICE_VIEW.network);
  const fw = network.find((d) => d.id === "opnsense");
  const ap = network.find((d) => d.id === "tplink");
  const modem = DEVICE_VIEW.map.find((d) => d.kind === "modem");
  primary.push(fw ? dApplianceCard(fw) : dInventoryCard({ name: "OPNsense firewall", address: "192.168.0.1", state: "unknown", role: "Firewall / gateway", detail: "Live firewall status unavailable", icon: "shield" }));
  primary.push(ap ? dApplianceCard(ap) : dInventoryCard({ name: "TP-Link access point", address: "192.168.0.2", state: "unknown", role: "Wi-Fi access point", detail: "Live router status unavailable", icon: "net" }));
  primary.push(dInventoryCard(modem || { name: "Internet modem", state: "unknown", role: "WAN upstream", detail: "Model and management address have not been inventoried.", icon: "net" }));
  const already = new Set([
    ...nodes.map((n) => n.ip),
    ...remotes.map((c) => deviceHost(c.connection.url)),
    ...network.map((d) => d.address),
    local.hostname
  ].filter(Boolean).map((x) => String(x).toLowerCase()));
  const inventory = DEVICE_VIEW.map.filter((d) => d.kind !== "modem" && !already.has(String(d.address || "").toLowerCase()) && !nodeNames.has(String(d.name || "").toLowerCase())).map((d) => ({
    name: d.name, address: d.address, state: d.state || "unknown", role: d.role, detail: d.detail, icon: d.icon, href: d.href
  }));
  const liveLeases = deviceRows(fw?.leases).filter((l) => l.address && !already.has(String(l.address).toLowerCase()) && !inventory.some((d) => deviceSame(d.address, l.address))).map((l) => ({
    name: l.hostname || "LAN client", address: l.address, state: "leased", role: "Active DHCP lease",
    detail: (l.lease_type === "static" ? "Static reservation" : "Lease present") + (l.hwaddr ? " · " + l.hwaddr : "")
  }));
  const allHostModels = [localModel].concat(nodeModels, otherHosts);
  const liveHosts = allHostModels.filter((m) => m.online);
  const cores = liveHosts.reduce((sum, m) => sum + (dNum(m.pveNode?.maxcpu) || dNum(m.info?.cores) || 0), 0);
  const ramTotal = liveHosts.reduce((sum, m) => sum + (dNum(m.pveNode?.maxmem) || dNum(m.info?.memory_total) || 0), 0);
  let cpuWeighted = 0, cpuWeight = 0, totalVram = 0, gpuCount = 0;
  for (const m of liveHosts) {
    const count = dNum(m.pveNode?.maxcpu) || dNum(m.info?.cores) || 0;
    const pct = m.pveNode ? (dNum(m.pveNode.cpu) === null ? null : dNum(m.pveNode.cpu) * 100) : dNum(m.srv?.cpu_usage_pct);
    if (pct !== null && count > 0) { cpuWeighted += pct * count; cpuWeight += count; }
    for (const g of dGpuRows(m.srv)) { gpuCount++; totalVram += dNum(g.vram_total_bytes) || 0; }
  }
  const cpuSummary = cpuWeight ? Math.round(cpuWeighted / cpuWeight) + "% avg" : "Unavailable";
  const netLive = network.filter((d) => d.online === true).length;
  const hostTotal = allHostModels.length;
  const hostOnline = liveHosts.length;
  const guestUp = guests.filter((g) => g.status === "running").length;
  const clusterName = "Proxmox cluster";
  const now = DEVICE_VIEW.updated ? new Date(DEVICE_VIEW.updated).toLocaleTimeString() : "now";
  const primaryCount = primary.length + inventory.length + liveLeases.length;
  const offlineCount = allHostModels.filter((m) => !m.online).length + network.filter((d) => d.online === false).length;
  $("#top-actions").innerHTML = '<button class="btn" id="devices-refresh">Refresh snapshot</button><button class="btn primary" id="devices-connect">Manage connections</button>';
  $("#devices-refresh").onclick = () => VIEWS.topology();
  $("#devices-connect").onclick = () => go("fleet");
  const metrics = [
    '<div class="card"><div class="label">LIVE HOSTS</div><div class="big">' + hostOnline + ' / ' + hostTotal + '</div><div class="muted">MWM or Proxmox node checks</div></div>',
    '<div class="card"><div class="label">CPU</div><div class="big">' + esc(cpuSummary) + '</div><div class="muted">' + cores + ' logical cores across live hosts</div></div>',
    '<div class="card"><div class="label">TOTAL RAM</div><div class="big">' + esc(dBytes(ramTotal)) + '</div><div class="muted">Installed memory across live hosts</div></div>',
    '<div class="card"><div class="label">GPU / VRAM</div><div class="big">' + (gpuCount ? gpuCount + " GPUs" : "No GPU telemetry") + '</div><div class="muted">' + (totalVram ? esc(dBytes(totalVram)) + " VRAM reported" : "VRAM totals unavailable") + '</div></div>',
    '<div class="card"><div class="label">PVE GUESTS</div><div class="big">' + (guests.length ? guestUp + " / " + guests.length : "Unavailable") + '</div><div class="muted">Cluster total counted once</div></div>',
    '<div class="card"><div class="label">NETWORK</div><div class="big">' + netLive + ' / ' + network.length + '</div><div class="muted">Firewall and access point live checks</div></div>'
  ].join("");
  $("#page").innerHTML = '<div class="device-hero card"><div><div class="label">REAL DEVICE SNAPSHOT · UPDATED ' + esc(now) + '</div><h2>All devices</h2><p>Hardware and live status from MWM agents, Proxmox, OPNsense and TP-Link. Each value is labeled by its source.</p></div><div class="row"><span class="pill ' + (offlineCount ? "medium" : "low") + '">' + offlineCount + ' offline / attention</span><button class="btn" data-go="cluster">Open Proxmox</button></div></div>' +
    '<div class="grid g3 device-snapshot-summary">' + metrics + '</div>' +
    dSection("Managed hosts and network equipment", "CPU utilization, load averages, cores, RAM, VRAM, disks, physical-drive reporting, uptime, firewall counters and device reachability.", primary) +
    dSection("Other LAN devices", "Devices without an MWM agent are included from inventory. DHCP records are labeled last-seen and are not treated as live checks.", inventory.concat(liveLeases).map(dInventoryCard)) +
    '<div class="device-foot muted">Snapshot time: ' + esc(DEVICE_VIEW.updated ? new Date(DEVICE_VIEW.updated).toLocaleString() : "Unavailable") + '. CPU usage is sampled briefly by MWM; load average, RAM, VRAM and disk readings come from each source. Missing fields are shown as unavailable rather than estimated.</div>';
  $$("[data-go]").forEach((b) => b.onclick = () => go(b.dataset.go));
  $$("[data-device-target]").forEach((b) => b.onclick = () => switchTo(b.dataset.deviceTarget));
}
async function deviceSource(cmd, valid) {
  const local = await deviceTry(() => invoke(cmd));
  if (valid(local)) return local;
  const ordered = DEVICE_VIEW.connections.slice().sort((a, b) => Number(deviceHost(b.url) === "192.168.0.6") - Number(deviceHost(a.url) === "192.168.0.6"));
  for (const c of ordered) {
    const v = await deviceTry(() => localInvoke("remote_call", { id: c.id, cmd, args: {} }));
    if (valid(v)) return v;
  }
  return local;
}
VIEWS.topology = async function () {
  $("#top-actions").innerHTML = '<button class="btn" disabled>Refreshing snapshot…</button>';
  $("#page").innerHTML = loading("Reading host, Proxmox, firewall and router metrics…");
  const [local, localSrv, connections] = await Promise.all([
    deviceTry(() => localInvoke("system_info")),
    deviceTry(() => localInvoke("server_info")),
    deviceTry(() => localInvoke("conn_list"))
  ]);
  DEVICE_VIEW.local = local || S.info || {};
  DEVICE_VIEW.localSrv = localSrv || {};
  DEVICE_VIEW.connections = Array.isArray(connections) ? connections : [];
  DEVICE_VIEW.remotes = await Promise.all(DEVICE_VIEW.connections.map(async (connection) => {
    const start = Date.now();
    const [info, srv] = await Promise.all([
      deviceTry(() => localInvoke("remote_call", { id: connection.id, cmd: "system_info", args: {} })),
      deviceTry(() => localInvoke("remote_call", { id: connection.id, cmd: "server_info", args: {} }))
    ]);
    return { connection, info, srv, online: !!info, ms: Date.now() - start };
  }));
  const [network, map] = await Promise.all([
    deviceSource("appliance_status", (v) => Array.isArray(v?.devices)),
    deviceSource("device_map", (v) => deviceRows(v).length > 0)
  ]);
  DEVICE_VIEW.network = network;
  DEVICE_VIEW.map = deviceRows(map);
  DEVICE_VIEW.updated = new Date().toISOString();
  if (current === "topology") drawDeviceTopology();
};