/* MWM - right-click (context) menus for every list in the app.
   One small menu component + per-page item builders, wired with a single
   delegated `contextmenu` listener. Keyboard: menu key / Shift+F10, arrows,
   Enter, Esc. */
"use strict";

const CTX = { el: null, items: [], idx: -1 };

function closeCtx() {
  CTX.el?.remove();
  CTX.el = null;
  CTX.items = [];
  CTX.idx = -1;
}

/** items: [{label, key?, icon?, danger?, disabled?, run}] or "-" for a divider. */
function ctxMenu(x, y, items) {
  closeCtx();
  const list = items.filter((i) => i && (i === "-" || !i.hidden));
  // Drop leading / trailing / doubled dividers.
  const clean = list.filter((it, i, a) => it !== "-" || (i > 0 && i < a.length - 1 && a[i - 1] !== "-"));
  if (!clean.length) return;
  const el = document.createElement("div");
  el.className = "ctx";
  el.setAttribute("role", "menu");
  el.innerHTML = clean.map((it, i) => it === "-" ? '<div class="ctx-sep"></div>'
    : `<button class="ctx-item ${it.danger ? "danger" : ""}" data-ci="${i}" ${it.disabled ? "disabled" : ""} role="menuitem"><span class="ctx-ico">${it.icon || ""}</span><span class="ctx-l">${esc(it.label)}</span><span class="ctx-k">${esc(it.key || "")}</span></button>`).join("");
  document.body.appendChild(el);
  // Keep the menu on screen.
  const r = el.getBoundingClientRect();
  el.style.left = `${Math.max(4, Math.min(x, innerWidth - r.width - 6))}px`;
  el.style.top = `${Math.max(4, Math.min(y, innerHeight - r.height - 6))}px`;
  CTX.el = el;
  CTX.items = clean;
  el.onclick = (e) => {
    const b = e.target.closest("[data-ci]");
    if (!b || b.disabled) return;
    const it = clean[+b.dataset.ci];
    closeCtx();
    Promise.resolve().then(() => it.run && it.run());
  };
}

function ctxFocus(delta) {
  const btns = [...(CTX.el?.querySelectorAll(".ctx-item:not([disabled])") || [])];
  if (!btns.length) return;
  CTX.idx = (CTX.idx + delta + btns.length) % btns.length;
  btns[CTX.idx].focus();
}

document.addEventListener("mousedown", (e) => { if (CTX.el && !e.target.closest(".ctx")) closeCtx(); }, true);
document.addEventListener("scroll", () => closeCtx(), true);
window.addEventListener("blur", closeCtx);
document.addEventListener("keydown", (e) => {
  if (!CTX.el) return;
  if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); closeCtx(); }
  else if (e.key === "ArrowDown") { e.preventDefault(); e.stopPropagation(); ctxFocus(1); }
  else if (e.key === "ArrowUp") { e.preventDefault(); e.stopPropagation(); ctxFocus(-1); }
}, true);

const isLocalDesktop = () => !!TAURI && !TARGET;
const fileName = (p) => p.split(/[\\/]/).filter(Boolean).pop() || p;
const parentOf = (p) => { const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/")); return i > 0 ? p.slice(0, i) || p : p; };
const canAnalyze = () => typeof openLab === "function";

/** Open a path in Commander's left pane (folder) or its parent with the file selected. */
function openInCommander(path, isDir) {
  go("files");
  const p = CMD.panes[0];
  CMD.active = 0;
  setTimeout(() => (isDir ? loadPane(p, path, false) : loadPane(p, parentOf(path), fileName(path))), 300);
}

// --------------------------------------------------------- Commander ----
function commanderItems(p, row) {
  const t = targets(p);
  const one = t.length === 1 ? t[0] : null;
  const isArchive = one && !one.is_dir && ARCHIVES.includes(one.ext);
  const n = t.length;
  const what = n > 1 ? `${n} items` : "";
  if (!row) {
    // Empty area of a pane.
    return [
      { label: "New folder", key: "F7", icon: "＋", run: () => runCmd("mkdir") },
      { label: CMD.clip ? `Paste ${CMD.clip.paths.length} item(s)${CMD.clip.move ? " (move)" : ""}` : "Paste", key: "Ctrl+V", icon: "📋", disabled: !CMD.clip, run: () => runCmd("cpaste") },
      "-",
      { label: "Select all", key: "Ctrl+A", run: () => { p.rows.forEach((r) => !r.up && p.sel.add(r.path)); drawPane(p); } },
      { label: "Refresh", key: "Ctrl+R", icon: "⟳", run: () => runCmd("refresh") },
      { label: CMD.hidden ? "Hide hidden files" : "Show hidden files", key: "Ctrl+H", run: () => runCmd("hidden") },
      "-",
      ...["name", "ext", "size", "date"].map((k) => ({ label: `Sort by ${{ name: "name", ext: "type", size: "size", date: "date" }[k]}${p.sort.key === k ? (p.sort.dir > 0 ? " ▴" : " ▾") : ""}`, run: () => { p.sort = { key: k, dir: p.sort.key === k ? -p.sort.dir : 1 }; buildRows(p); drawPane(p); } })),
      "-",
      { label: "Calculate folder sizes", run: () => runCmd("sizes") },
      { label: "Compare with other pane", run: () => runCmd("compare") },
      { label: "Search here...", key: "Alt+F7", icon: "🔍", run: () => runCmd("search") },
      { label: "Open terminal here", hidden: !isLocalDesktop(), run: () => runCmd("term") },
      { label: "Show in Explorer", hidden: !isLocalDesktop(), run: () => invoke("reveal", { path: p.path }) },
      { label: "Copy folder path", run: () => copyText(p.path) },
    ];
  }
  if (row.up) return [{ label: "Go up", key: "Backspace", run: () => goUp(p) }];
  return [
    { label: one?.is_dir ? "Open folder" : "Open", key: "Enter", icon: one?.is_dir ? "📂" : "▶", hidden: !one, run: () => openCursor(p) },
    { label: "Open in other pane", hidden: !one?.is_dir, run: () => loadPane(other(), one.path, false) },
    { label: "View", key: "F3", icon: "👁", hidden: !one || one.is_dir, run: () => runCmd("view") },
    { label: "Edit", key: "F4", icon: "✎", hidden: !one || one.is_dir || !isLocalDesktop(), run: () => runCmd("edit") },
    { label: "Open with default app", hidden: !one || one.is_dir || !isLocalDesktop(), run: () => invoke("open_default", { path: one.path }) },
    { label: "Analyze for malware", icon: "🛡", hidden: !one || one.is_dir || !canAnalyze(), run: () => openLab(one.path) },
    "-",
    { label: `Copy ${what}`.trim(), key: "Ctrl+C", icon: "⧉", run: () => runCmd("ccopy") },
    { label: `Cut ${what}`.trim(), key: "Ctrl+X", icon: "✂", run: () => runCmd("ccut") },
    { label: CMD.clip ? `Paste ${CMD.clip.paths.length} item(s) here` : "Paste", key: "Ctrl+V", icon: "📋", disabled: !CMD.clip, run: () => runCmd("cpaste") },
    { label: `Copy ${what || "to other pane"}${what ? " to other pane" : ""}`, key: "F5", run: () => runCmd("copy") },
    { label: `Move ${what || "to other pane"}${what ? " to other pane" : ""}`, key: "F6", run: () => runCmd("move") },
    "-",
    { label: "Rename", key: "F2", hidden: !one, run: () => runCmd("rename") },
    { label: `Multi-rename ${n} items`, hidden: n < 2, run: () => runCmd("mrename") },
    { label: "Copy path", run: () => copyText(t.map((x) => x.path).join("\n")) },
    { label: "Copy name", hidden: !one, run: () => copyText(one.name) },
    "-",
    { label: `Pack ${what || one?.name || ""} to .zip`, icon: "🗜", run: () => runCmd("pack") },
    { label: "Extract here", hidden: !isArchive, run: () => runCmd("unpackhere") },
    { label: "Extract to other pane...", hidden: !isArchive, run: () => runCmd("unpack") },
    { label: "Calculate size", hidden: !t.some((x) => x.is_dir), run: () => runCmd("sizes") },
    "-",
    { label: "Show in Explorer", hidden: !isLocalDesktop(), run: () => invoke("reveal", { path: (one || t[0]).path }) },
    { label: "Open terminal here", hidden: !isLocalDesktop() || !one?.is_dir, run: () => invoke("terminal", { dir: one.path }) },
    { label: "Properties", key: "Alt+Enter", hidden: !one, run: () => runCmd("props") },
    "-",
    { label: `Move ${what || "to Recycle Bin"}${what ? " to Recycle Bin" : ""}`, key: "F8", icon: "🗑", danger: true, run: () => runCmd("delete") },
    { label: "Delete permanently", key: "Shift+Del", danger: true, run: () => runCmd("delete!") },
  ];
}

// Extra Commander commands used by the menu / keyboard.
const _runCmd = runCmd;
runCmd = async function (c) {
  const p = active();
  if (c === "ccopy" || c === "ccut") {
    const t = targets(p);
    if (!t.length) return;
    CMD.clip = { paths: t.map((x) => x.path), move: c === "ccut" };
    toast(`${t.length} item(s) ${c === "ccut" ? "cut" : "copied"} - paste with Ctrl+V in any folder.`);
    return;
  }
  if (c === "cpaste") {
    if (!CMD.clip) return;
    const { paths, move } = CMD.clip;
    const clash = (await guard(() => invoke("files_conflicts", { sources: paths, dest: p.path }))) || [];
    let mode = "overwrite";
    if (clash.length) {
      mode = await choose(`${clash.length} item(s) already exist here`, `<div class="mono muted" style="max-height:30vh;overflow:auto">${clash.slice(0, 50).map(esc).join("<br>")}</div>`, [["skip", "Skip those"], ["rename", "Keep both"], ["overwrite", "Overwrite", "danger"]]);
      if (!mode) return;
    }
    const id = await invoke("files_transfer", { sources: paths, dest: p.path, mode, isMove: move });
    if (move) CMD.clip = null;
    watchJob(id, () => Promise.all(CMD.panes.map((x) => loadPane(x, x.path, cur(x)?.name))));
    return;
  }
  if (c === "unpackhere") {
    const r = cur(p);
    if (!r || r.is_dir) return;
    const dest = joinPath(p.path, r.name.replace(/\.(zip|tar|gz|tgz|bz2|xz|7z)$/gi, ""));
    const id = await invoke("files_unpack", { archive: r.path, dest });
    watchJob(id, () => loadPane(p, p.path, fileName(dest)));
    return;
  }
  return _runCmd(c);
};

// Ctrl+C / X / V and the menu key inside Commander.
document.addEventListener("keydown", (e) => {
  if (current !== "files" || modalOpen() || CTX.el) return;
  if (e.target.matches?.("input, textarea, select")) return;
  const k = e.key.toLowerCase();
  if ((e.ctrlKey || e.metaKey) && ["c", "x", "v"].includes(k)) {
    e.preventDefault();
    e.stopPropagation();
    runCmd({ c: "ccopy", x: "ccut", v: "cpaste" }[k]);
  } else if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
    e.preventDefault();
    e.stopPropagation();
    const p = active();
    const tr = $(`#list${p.i} tr[data-i="${p.cursor}"]`);
    const r = tr?.getBoundingClientRect();
    ctxMenu(r ? r.left + 60 : 300, r ? r.bottom : 300, commanderItems(p, p.rows[p.cursor]));
  }
}, true);

// ------------------------------------------------------ other pages ----
function rowMenu(e) {
  const tr = e.target.closest("tr, [data-drill], [data-reveal], .hit");
  // Commander
  const pane = e.target.closest(".pane");
  if (current === "files" && pane) {
    const p = CMD.panes[+pane.dataset.pane];
    activate(p.i);
    const rowEl = e.target.closest("tr[data-i]");
    if (rowEl) {
      const i = +rowEl.dataset.i;
      const r = p.rows[i];
      // Right-clicking outside the selection targets just that row (Explorer behaviour).
      if (r && !p.sel.has(r.path) && p.sel.size) { p.sel.clear(); drawPane(p); }
      setCursor(p, i);
      return commanderItems(p, r);
    }
    return e.target.closest(".pane-list") ? commanderItems(p, null) : null;
  }
  if (current === "disk") {
    const el = e.target.closest("[data-drill], [data-reveal]");
    if (!el) return null;
    const path = el.dataset.drill || el.dataset.reveal || "";
    const isDir = !!el.dataset.drill;
    if (!path) return null;
    return [
      { label: isDir ? "Analyze this folder" : "Open", icon: "📊", run: () => (isDir ? (diskRoot = path, $("#dk-go").click()) : invoke("open_default", { path })) },
      { label: "Open in Commander", icon: "📂", run: () => openInCommander(path, isDir) },
      { label: "Show in Explorer", hidden: !isLocalDesktop(), run: () => invoke("reveal", { path }) },
      { label: "Analyze for malware", icon: "🛡", hidden: isDir || !canAnalyze(), run: () => openLab(path) },
      { label: "Copy path", run: () => copyText(path) },
      "-",
      { label: "Move to Recycle Bin", icon: "🗑", danger: true, run: async () => {
        if (!(await choose(`Recycle ${esc(fileName(path))}?`, `<p class="muted mono">${esc(path)}</p>`, [["go", "Recycle", "danger"]]))) return;
        const id = await invoke("files_delete", { paths: [path], permanent: false });
        watchJob(id, () => { if (current === "disk") $("#dk-go")?.click(); });
      } },
    ];
  }
  if (current === "dupes") {
    const cb = tr?.querySelector?.("[data-dup]");
    if (!cb) return null;
    const path = cb.dataset.dup;
    const g = dupGroups?.find((x) => x.files.some((f) => f.path === path));
    return [
      { label: "Open", hidden: !isLocalDesktop(), run: () => invoke("open_default", { path }) },
      { label: "Open in Commander", icon: "📂", run: () => openInCommander(path, false) },
      { label: "Show in Explorer", hidden: !isLocalDesktop(), run: () => invoke("reveal", { path }) },
      { label: "Copy path", run: () => copyText(path) },
      "-",
      { label: "Keep only this copy", icon: "✔", hidden: !g, run: () => { g.files.forEach((f) => (f.path === path ? dupDel.delete(f.path) : dupDel.add(f.path))); drawDupes(); } },
      { label: dupDel.has(path) ? "Keep this copy" : "Mark this copy for removal", run: () => { dupDel.has(path) ? dupDel.delete(path) : dupDel.add(path); drawDupes(); } },
    ];
  }
  if (current === "uninstaller") {
    const id = tr?.dataset?.app;
    const app = id && S.apps?.find((a) => a.id === id);
    if (!app) return null;
    appSel = id;
    drawApps();
    const loc = (app.install_location || "").replace(/^"|"$/g, "");
    return [
      { label: "Uninstall...", icon: "🗑", danger: true, disabled: !app.can_uninstall, run: () => uninstallFlow(app) },
      { label: "Scan leftovers only", run: () => leftoverFlow(app, false) },
      "-",
      { label: "Open install folder in Commander", icon: "📂", hidden: !loc, run: () => openInCommander(loc, true) },
      { label: "Show install folder in Explorer", hidden: !loc || !isLocalDesktop(), run: () => invoke("reveal", { path: loc }) },
      { label: "Search the web for this program", hidden: !isLocalDesktop(), run: () => invoke("open_default", { path: `https://duckduckgo.com/?q=${encodeURIComponent(app.name + " " + app.publisher)}` }) },
      { label: "Copy name", run: () => copyText(app.name) },
      { label: "Copy uninstall command", hidden: !app.uninstall_cmd, run: () => copyText(app.uninstall_cmd) },
    ];
  }
  if (current === "startup") {
    const tg = tr?.querySelector?.("[data-toggle]");
    const item = tg && S.startup?.find((i) => i.id === tg.dataset.toggle);
    if (!item) return null;
    const exe = (item.command.match(/^"([^"]+)"/) || [null, item.command.split(/\s+/)[0]])[1] || "";
    return [
      { label: item.enabled ? "Disable at startup" : "Enable at startup", icon: item.enabled ? "⏻" : "▶", run: () => tg.click() },
      "-",
      { label: "Open file location", hidden: !exe || !isLocalDesktop(), run: () => invoke("reveal", { path: exe }) },
      { label: "Open in Commander", hidden: !exe, run: () => openInCommander(exe, false) },
      { label: "Analyze for malware", icon: "🛡", hidden: !exe || !canAnalyze(), run: () => openLab(exe) },
      { label: "Copy command", run: () => copyText(item.command) },
    ];
  }
  if (current === "perf") {
    const kb = tr?.querySelector?.("[data-kill]");
    if (!kb) return null;
    const exe = tr.querySelector(".cell-sub")?.textContent?.trim() || "";
    return [
      { label: "End task", icon: "✖", danger: true, run: () => kb.click() },
      "-",
      { label: "Open file location", hidden: !exe || !isLocalDesktop(), run: () => invoke("reveal", { path: exe }) },
      { label: "Analyze for malware", icon: "🛡", hidden: !exe || !canAnalyze(), run: () => openLab(exe) },
      { label: "Copy path", hidden: !exe, run: () => copyText(exe) },
    ];
  }
  if (current === "keys") {
    const kc = tr?.querySelector?.("[data-kcopy]");
    if (!kc) return null;
    const i = +kc.dataset.kcopy;
    const ks = tr.querySelector("[data-kshow]");
    return [
      { label: "Copy", icon: "⧉", run: () => copyText(KEYS.list[i].value) },
      { label: KEYS.shown.has(i) ? "Hide" : "Show", hidden: !ks, run: () => ks.click() },
      { label: "Copy name + key", run: () => copyText(`${KEYS.list[i].name}: ${KEYS.list[i].value}`) },
    ];
  }
  if (current === "cluster") {
    const row = e.target.closest("[data-guest]");
    if (!row) return null;
    const [node, kind, vmid] = row.dataset.guest.split("|");
    const g = arr(PVE.ov?.guests).find((x) => String(x.vmid) === vmid) || {};
    const name = g.name || vmid;
    const running = g.status === "running";
    return [
      { label: "Details...", icon: "🔎", run: () => guestPanel(node, kind, vmid) },
      "-",
      { label: "Start", icon: "▶", hidden: running, run: () => guestAction(node, kind, vmid, name, "start") },
      { label: "Shut down", icon: "⏻", hidden: !running, run: () => guestAction(node, kind, vmid, name, "shutdown") },
      { label: "Reboot", icon: "⟳", hidden: !running, run: () => guestAction(node, kind, vmid, name, "reboot") },
      { label: "Force stop", danger: true, hidden: !running, run: () => guestAction(node, kind, vmid, name, "stop") },
      "-",
      { label: "Snapshot now...", icon: "📸", run: () => snapshotDialog(node, kind, vmid, name) },
      { label: "Back up now...", icon: "💾", run: () => backupDialog(node, kind, vmid, name) },
      { label: "Migrate to another node...", icon: "⇄", run: () => migrateDialog(node, kind, vmid, name, running) },
      { label: "Docker containers...", icon: "🐳", hidden: kind !== "lxc" || !running, run: () => dockerPanel(node, vmid, name) },
      "-",
      { label: `Show only ${node}`, run: () => { PVE.node = node; drawCluster(); } },
      { label: "Copy ID", run: () => copyText(vmid) },
      { label: "Copy name", run: () => copyText(name) },
    ];
  }
  if (current === "plugins") {
    const row = e.target.closest("[data-plugin]");
    return row ? pluginMenu(PLUG.rows[+row.dataset.plugin]) : null;
  }
  if (current === "server") {
    const dk = tr?.querySelector?.("[data-dkct]");
    const act = tr?.querySelector?.("[data-srv]");
    if (!dk && !act) return null;
    const btns = [...tr.querySelectorAll("[data-srv]")];
    return [
      { label: "Docker containers...", icon: "🐳", hidden: !dk, run: () => dk.click() },
      ...btns.map((b) => ({ label: b.textContent.trim(), danger: b.classList.contains("danger"), run: () => b.click() })),
    ];
  }
  return null;
}

// Capture the event before a row-specific WebView handler can consume it.
// WebView2 needs preventDefault on this event to suppress its own menu.
document.addEventListener("contextmenu", (e) => {
  // Text fields keep the normal copy/paste menu.
  if (e.target.matches?.("input, textarea") || e.target.closest(".keyval, .viewer, .console")) return;
  const items = rowMenu(e);
  e.preventDefault();
  if (items) ctxMenu(e.clientX, e.clientY, items);
  else closeCtx();
}, true);
