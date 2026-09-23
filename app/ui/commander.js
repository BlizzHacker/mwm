/* MWM Commander - dual-pane, keyboard-first file manager in the spirit of
   Double Commander / Total Commander / Geek Squad's FMOD. Independent code.
   Keys: Tab switch pane, Enter open, Backspace up, Space/Ins select, Ctrl+A all,
   F2 rename, F3 view, F4 edit, F5 copy, F6 move, F7 new folder, F8/Del recycle,
   Shift+Del delete forever, Alt+F7 search, Ctrl+R refresh, Ctrl+H hidden,
   Ctrl+U swap panes, Alt+Enter properties, type letters to jump. */
"use strict";

const CMD = {
  active: 0,
  hidden: (() => { try { return localStorage.getItem("mwm.hidden") === "1"; } catch { return false; } })(),
  roots: [],
  panes: [0, 1].map((i) => ({
    i,
    path: (() => { try { return localStorage.getItem(`mwm.pane${i}`) || ""; } catch { return ""; } })(),
    listing: null,
    rows: [],
    sel: new Set(),
    cursor: 0,
    sort: { key: "name", dir: 1 },
    sizes: {},
    error: "",
  })),
  typed: "",
  typedAt: 0,
};

const isWin = () => (S.info?.platform || "windows") === "windows";
const sep = () => (isWin() ? "\\" : "/");
const joinPath = (dir, name) => (dir.endsWith(sep()) ? dir + name : dir + sep() + name);
const fmtDate = (s) => (s ? new Date(s * 1000).toLocaleString(undefined, { year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" }) : "");
const ARCHIVES = ["zip", "tar", "gz", "tgz", "bz2", "xz", "7z"];

function fileGlyph(e) {
  if (e.up) return "↰";
  if (e.is_dir) return "📁";
  const x = e.ext;
  if (["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico"].includes(x)) return "🖼";
  if (["mp4", "mkv", "avi", "mov", "webm"].includes(x)) return "🎞";
  if (["mp3", "flac", "wav", "ogg", "m4a"].includes(x)) return "🎵";
  if (ARCHIVES.includes(x) || x === "rar" || x === "iso") return "🗜";
  if (["exe", "msi", "bat", "cmd", "ps1", "sh", "appimage"].includes(x)) return "⚙";
  if (["txt", "md", "log", "ini", "cfg", "json", "xml", "yml", "yaml", "toml", "csv"].includes(x)) return "📄";
  return "▫";
}

// ------------------------------------------------------------ loading ----
async function loadPane(p, path, keepCursorName) {
  try {
    const l = await invoke("files_list", { dir: path });
    p.listing = l;
    p.path = l.path;
    p.error = "";
    p.sel.clear();
    p.sizes = {};
    try { localStorage.setItem(`mwm.pane${p.i}`, p.path); } catch {}
  } catch (e) {
    p.error = String(e);
    if (!p.listing) p.listing = { path, parent: null, entries: [], free: 0 };
  }
  buildRows(p);
  // string = put the cursor on that name; false = top; undefined = stay put.
  if (typeof keepCursorName === "string") {
    const idx = p.rows.findIndex((r) => r.name === keepCursorName);
    p.cursor = idx >= 0 ? idx : Math.min(p.cursor, Math.max(0, p.rows.length - 1));
  } else if (keepCursorName === false) p.cursor = 0;
  else p.cursor = Math.min(p.cursor, Math.max(0, p.rows.length - 1));
  if (current === "files") drawPane(p);
}

function buildRows(p) {
  const l = p.listing;
  let list = l.entries.filter((e) => CMD.hidden || !e.hidden);
  const { key, dir } = p.sort;
  const val = (e) => (key === "size" ? (e.is_dir ? p.sizes[e.path] ?? -1 : e.size) : key === "date" ? e.modified : key === "ext" ? e.ext : e.name.toLowerCase());
  list.sort((a, b) => (b.is_dir - a.is_dir) || (val(a) < val(b) ? -dir : val(a) > val(b) ? dir : a.name.localeCompare(b.name)));
  p.rows = (l.parent ? [{ up: true, name: "..", path: l.parent, is_dir: true, size: 0, modified: 0, ext: "" }] : []).concat(list);
}

const cur = (p = CMD.panes[CMD.active]) => p.rows[p.cursor];
const other = () => CMD.panes[1 - CMD.active];
const active = () => CMD.panes[CMD.active];
/** Selected items, or the one under the cursor. Never includes "..". */
function targets(p = active()) {
  const sel = p.rows.filter((r) => !r.up && p.sel.has(r.path));
  if (sel.length) return sel;
  const c = cur(p);
  return c && !c.up ? [c] : [];
}

// ------------------------------------------------------------ drawing ----
VIEWS.files = async function () {
  $("#top-actions").innerHTML = `
    <button class="btn small" data-cmd="search" title="Alt+F7">${icon("search")}Search</button>
    <button class="btn small" data-cmd="compare" title="Mark files that differ between panes">Compare</button>
    <button class="btn small" data-cmd="mrename" title="Rename many files with a pattern">Multi-rename</button>
    <button class="btn small" data-cmd="pack">Pack .zip</button>
    <button class="btn small" data-cmd="unpack">Unpack</button>
    <button class="btn small" data-cmd="sizes" title="Calculate folder sizes">Sizes</button>
    <button class="btn small" data-cmd="term" title="Open a terminal here">Terminal</button>`;
  $("#page").innerHTML = `<div class="cmd">
      <div class="cmd-drives" id="cmd-drives"></div>
      <div class="panes">${[0, 1].map((i) => `<div class="pane" id="pane${i}" data-pane="${i}">
        <div class="pane-head"><button class="btn small ghost" data-up="${i}" title="Up (Backspace)">↑</button><input class="pathbox mono" id="path${i}" spellcheck="false"></div>
        <div class="pane-list" id="list${i}"></div>
        <div class="pane-foot" id="foot${i}"></div></div>`).join("")}</div>
      <div class="fkeys">${[["F2", "Rename", "rename"], ["F3", "View", "view"], ["F4", "Edit", "edit"], ["F5", "Copy", "copy"], ["F6", "Move", "move"], ["F7", "New folder", "mkdir"], ["F8", "Delete", "delete"], ["Alt+Enter", "Properties", "props"], ["Ctrl+H", CMD.hidden ? "Hide hidden" : "Show hidden", "hidden"]]
        .map(([k, t, c]) => `<button class="fk" data-cmd="${c}"><kbd>${k}</kbd>${t}</button>`).join("")}</div>
    </div>`;
  if (!CMD.roots.length) CMD.roots = (await guard(() => invoke("files_roots"))) || [];
  drawDrives();
  const home = CMD.roots.find((r) => r.kind === "place")?.path || CMD.roots[0]?.path || "C:\\";
  await Promise.all(CMD.panes.map((p, i) => (p.listing ? Promise.resolve() : loadPane(p, p.path || (i === 0 ? home : CMD.roots.find((r) => r.kind !== "place")?.path || home)))));
  if (current !== "files") return;
  CMD.panes.forEach(drawPane);
  wirePanes();
};

function drawDrives() {
  const el = $("#cmd-drives");
  if (!el) return;
  el.innerHTML = CMD.roots.map((r) => {
    const used = r.total ? 1 - r.free / r.total : 0;
    return `<button class="drive" data-root="${esc(r.path)}" title="${esc(r.label)}${r.total ? ` - ${bytes(r.free)} free of ${bytes(r.total)}` : ""}">
      <span>${r.kind === "place" ? "★" : r.kind === "network" ? "🖧" : r.kind === "removable" ? "⏏" : "🖴"} ${esc(r.kind === "place" ? r.label : r.path.replace(/\\$/, ""))}</span>
      ${r.total ? `<i class="drive-bar ${used > 0.93 ? "full" : ""}"><b style="width:${(used * 100).toFixed(0)}%"></b></i>` : ""}</button>`;
  }).join("");
}

function drawPane(p) {
  const list = $(`#list${p.i}`);
  if (!list) return;
  $(`#pane${p.i}`).classList.toggle("active", p.i === CMD.active);
  const pb = $(`#path${p.i}`);
  if (document.activeElement !== pb) pb.value = p.path;
  const arrow = (k) => (p.sort.key === k ? (p.sort.dir > 0 ? " ▴" : " ▾") : "");
  list.innerHTML = p.error && !p.rows.length
    ? `<div class="empty"><h3>Can't open this folder</h3>${esc(p.error)}</div>`
    : `<table class="table ftable"><thead><tr><th class="sort" data-sortk="name">Name${arrow("name")}</th><th class="sort" data-sortk="ext" style="width:62px">Ext${arrow("ext")}</th><th class="sort num" data-sortk="size" style="width:92px">Size${arrow("size")}</th><th class="sort" data-sortk="date" style="width:138px">Modified${arrow("date")}</th></tr></thead>
      <tbody>${p.rows.map((r, i) => rowHtml(p, r, i)).join("")}</tbody></table>`;
  drawFoot(p);
  scrollCursor(p);
}

function rowHtml(p, r, i) {
  const size = r.up ? "" : r.is_dir ? (p.sizes[r.path] !== undefined ? bytes(p.sizes[r.path]) : "&lt;DIR&gt;") : bytes(r.size);
  const name = r.is_dir || !r.ext ? r.name : r.name.slice(0, -(r.ext.length + 1));
  return `<tr data-i="${i}" class="${i === p.cursor ? "cur" : ""} ${p.sel.has(r.path) ? "picked" : ""} ${r.hidden ? "hid" : ""} ${r.is_dir ? "dir" : ""}">
    <td class="fname"><span class="glyph">${fileGlyph(r)}</span>${esc(name)}</td><td class="muted">${r.is_dir ? "" : esc(r.ext)}</td><td class="num">${size}</td><td class="muted">${r.up ? "" : fmtDate(r.modified)}</td></tr>`;
}

function drawFoot(p) {
  const f = $(`#foot${p.i}`);
  if (!f) return;
  const picked = p.rows.filter((r) => p.sel.has(r.path));
  const pb = picked.reduce((a, r) => a + (r.is_dir ? p.sizes[r.path] || 0 : r.size), 0);
  const files = p.rows.filter((r) => !r.up && !r.is_dir);
  f.innerHTML = `<span>${picked.length ? `<b>${picked.length}</b> selected (${bytes(pb)}) of ` : ""}${files.length} files, ${p.rows.filter((r) => r.is_dir && !r.up).length} folders</span><span>${p.listing?.free ? `${bytes(p.listing.free)} free` : ""}${p.error ? ` <span class="danger-t">${esc(p.error)}</span>` : ""}</span>`;
}

function setCursor(p, i) {
  const list = $(`#list${p.i}`);
  i = Math.max(0, Math.min(p.rows.length - 1, i));
  list?.querySelector(`tr[data-i="${p.cursor}"]`)?.classList.remove("cur");
  p.cursor = i;
  list?.querySelector(`tr[data-i="${i}"]`)?.classList.add("cur");
  scrollCursor(p);
}
function scrollCursor(p) {
  $(`#list${p.i}`)?.querySelector(`tr[data-i="${p.cursor}"]`)?.scrollIntoView({ block: "nearest" });
}
function toggleSel(p, i) {
  const r = p.rows[i];
  if (!r || r.up) return;
  p.sel.has(r.path) ? p.sel.delete(r.path) : p.sel.add(r.path);
  $(`#list${p.i}`)?.querySelector(`tr[data-i="${i}"]`)?.classList.toggle("picked", p.sel.has(r.path));
  drawFoot(p);
}
function activate(i) {
  if (CMD.active === i) return;
  CMD.active = i;
  CMD.panes.forEach((p) => $(`#pane${p.i}`)?.classList.toggle("active", p.i === i));
}

// ------------------------------------------------------------- events ----
function wirePanes() {
  CMD.panes.forEach((p) => {
    const list = $(`#list${p.i}`);
    list.onmousedown = (e) => {
      activate(p.i);
      const tr = e.target.closest("tr[data-i]");
      if (!tr) return;
      const i = +tr.dataset.i;
      if (e.ctrlKey || e.metaKey) toggleSel(p, i);
      else if (e.shiftKey) {
        const [a, b] = [Math.min(p.cursor, i), Math.max(p.cursor, i)];
        for (let k = a; k <= b; k++) if (!p.rows[k].up) p.sel.add(p.rows[k].path);
        drawPane(p);
      } else if (e.button === 2) toggleSel(p, i);
      setCursor(p, i);
    };
    list.ondblclick = (e) => { if (e.target.closest("tr[data-i]")) openCursor(p); };
    list.oncontextmenu = (e) => e.preventDefault();
    list.onclick = (e) => {
      const th = e.target.closest("[data-sortk]");
      if (!th) return;
      const k = th.dataset.sortk;
      p.sort = { key: k, dir: p.sort.key === k ? -p.sort.dir : 1 };
      const name = cur(p)?.name;
      buildRows(p);
      p.cursor = Math.max(0, p.rows.findIndex((r) => r.name === name));
      drawPane(p);
    };
    const pb = $(`#path${p.i}`);
    pb.onkeydown = (e) => {
      if (e.key === "Enter") { loadPane(p, pb.value.trim(), false); pb.blur(); }
      if (e.key === "Escape") { pb.value = p.path; pb.blur(); }
    };
    pb.onfocus = () => activate(p.i);
  });
  $$("[data-up]").forEach((b) => (b.onclick = () => { activate(+b.dataset.up); goUp(active()); }));
  $$("[data-root]").forEach((b) => (b.onclick = () => loadPane(active(), b.dataset.root, false)));
}

function goUp(p) {
  if (!p.listing?.parent) return;
  const from = p.path.split(/[\\/]/).filter(Boolean).pop();
  loadPane(p, p.listing.parent, from);
}

function openCursor(p) {
  const r = cur(p);
  if (!r) return;
  if (r.up) return goUp(p);
  if (r.is_dir) return loadPane(p, r.path, false);
  guard(() => invoke("open_default", { path: r.path }));
}

document.addEventListener("click", (e) => {
  const b = e.target.closest("[data-cmd]");
  if (b && current === "files") runCmd(b.dataset.cmd);
});

document.addEventListener("keydown", (e) => {
  if (current !== "files" || modalOpen()) return;
  if (e.target.matches?.("input, textarea, select")) return;
  const p = active();
  const k = e.key;
  const handled = () => { e.preventDefault(); e.stopPropagation(); };
  const pageRows = Math.max(1, Math.floor(($(`#list${p.i}`)?.clientHeight || 400) / 30) - 1);
  if (k === "ArrowDown") { handled(); setCursor(p, p.cursor + 1); }
  else if (k === "ArrowUp") { handled(); setCursor(p, p.cursor - 1); }
  else if (k === "PageDown") { handled(); setCursor(p, p.cursor + pageRows); }
  else if (k === "PageUp") { handled(); setCursor(p, p.cursor - pageRows); }
  else if (k === "Home") { handled(); setCursor(p, 0); }
  else if (k === "End") { handled(); setCursor(p, p.rows.length - 1); }
  else if (k === "Tab") { handled(); activate(1 - CMD.active); }
  else if (k === "Enter" && e.altKey) { handled(); runCmd("props"); }
  else if (k === "Enter") { handled(); openCursor(p); }
  else if (k === "Backspace") { handled(); goUp(p); }
  else if (k === " " || k === "Insert") { handled(); toggleSel(p, p.cursor); setCursor(p, p.cursor + 1); }
  else if (k === "F2") { handled(); runCmd("rename"); }
  else if (k === "F3") { handled(); runCmd("view"); }
  else if (k === "F4") { handled(); runCmd("edit"); }
  else if (k === "F5") { handled(); runCmd("copy"); }
  else if (k === "F6") { handled(); runCmd("move"); }
  else if (k === "F7" && e.altKey) { handled(); runCmd("search"); }
  else if (k === "F7") { handled(); runCmd("mkdir"); }
  else if (k === "F8" || k === "Delete") { handled(); runCmd(e.shiftKey ? "delete!" : "delete"); }
  else if ((e.ctrlKey || e.metaKey) && k.toLowerCase() === "a") { handled(); p.rows.forEach((r) => !r.up && p.sel.add(r.path)); drawPane(p); }
  else if ((e.ctrlKey || e.metaKey) && k.toLowerCase() === "r") { handled(); runCmd("refresh"); }
  else if ((e.ctrlKey || e.metaKey) && k.toLowerCase() === "h") { handled(); runCmd("hidden"); }
  else if ((e.ctrlKey || e.metaKey) && k.toLowerCase() === "u") { handled(); runCmd("swap"); }
  else if (k.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey) {
    // Type-ahead: jump to the first name starting with what was typed.
    handled();
    const now = Date.now();
    CMD.typed = (now - CMD.typedAt < 900 ? CMD.typed : "") + k.toLowerCase();
    CMD.typedAt = now;
    const i = p.rows.findIndex((r) => !r.up && r.name.toLowerCase().startsWith(CMD.typed));
    if (i >= 0) setCursor(p, i);
  }
});

document.addEventListener("mwm-modal-closed", () => { if (current === "files") document.activeElement?.blur(); });

// ----------------------------------------------------------- commands ----
const refreshBoth = () => Promise.all(CMD.panes.map((p) => loadPane(p, p.path, cur(p)?.name)));

async function runCmd(c) {
  const p = active();
  const o = other();
  const t = targets(p);
  const names = (list) => (list.length === 1 ? `"${esc(list[0].name)}"` : `${list.length} items`);
  switch (c) {
    case "refresh": return refreshBoth();
    case "hidden":
      CMD.hidden = !CMD.hidden;
      try { localStorage.setItem("mwm.hidden", CMD.hidden ? "1" : "0"); } catch {}
      CMD.panes.forEach((x) => { buildRows(x); });
      return VIEWS.files();
    case "swap": {
      const [a, b] = [CMD.panes[0].path, CMD.panes[1].path];
      await Promise.all([loadPane(CMD.panes[0], b, false), loadPane(CMD.panes[1], a, false)]);
      return;
    }
    case "term": return guard(() => invoke("terminal", { dir: p.path }));
    case "view": {
      const r = cur(p);
      if (!r || r.up) return;
      if (r.is_dir) return loadPane(p, r.path, false);
      modal(loading(`Opening ${r.name}...`));
      const pv = await guard(() => invoke("files_preview", { path: r.path }));
      if (!pv) return closeModal();
      const body = pv.kind === "image" ? `<div class="viewer-img"><img src="${pv.content}" alt=""></div>`
        : `<pre class="viewer mono">${esc(pv.content)}</pre>`;
      modal(`<div class="spread"><h2 style="margin:0">${esc(r.name)}</h2><span class="muted">${bytes(pv.size)}${pv.truncated ? " - showing the start" : ""}</span></div>${body}
        <div class="row" style="justify-content:flex-end;margin-top:14px"><button class="btn" id="v-open">Open with default app</button><button class="btn primary" data-close>Close (Esc)</button></div>`);
      $("#modal-card").classList.add("wide");
      $("#v-open").onclick = () => invoke("open_default", { path: r.path });
      return;
    }
    case "edit": {
      const r = cur(p);
      if (r && !r.up && !r.is_dir) guard(() => invoke("edit_file", { path: r.path }));
      return;
    }
    case "mkdir": {
      const name = await ask("New folder", `Create in ${p.path}`, "", "Create");
      if (!name) return;
      const made = await guard(() => invoke("files_mkdir", { parent: p.path, name }));
      if (made) loadPane(p, p.path, name.split(/[\\/]/)[0]);
      return;
    }
    case "rename": {
      const r = cur(p);
      if (!r || r.up) return;
      const name = await ask("Rename", r.path, r.name, "Rename");
      if (!name || name === r.name) return;
      const ok = await guard(() => invoke("files_rename", { path: r.path, name }));
      if (ok) loadPane(p, p.path, name);
      return;
    }
    case "copy":
    case "move": {
      if (!t.length) return;
      const isMove = c === "move";
      const dest = await ask(`${isMove ? "Move" : "Copy"} ${t.length === 1 ? t[0].name : t.length + " items"}`, "To folder", o.path, isMove ? "Move" : "Copy");
      if (!dest) return;
      const sources = t.map((x) => x.path);
      const clash = await guard(() => invoke("files_conflicts", { sources, dest })) || [];
      let mode = "overwrite";
      if (clash.length) {
        mode = await choose(`${clash.length} item${clash.length > 1 ? "s" : ""} already exist${clash.length > 1 ? "" : "s"}`,
          `<div class="mono muted" style="max-height:30vh;overflow:auto">${clash.slice(0, 50).map(esc).join("<br>")}</div>`,
          [["skip", "Skip those"], ["rename", "Keep both"], ["overwrite", "Overwrite", "danger"]]);
        if (!mode) return;
      }
      const id = await invoke("files_transfer", { sources, dest, mode, isMove });
      p.sel.clear();
      watchJob(id, () => refreshBoth());
      return;
    }
    case "delete":
    case "delete!": {
      if (!t.length) return;
      const forever = c === "delete!";
      const ok = await choose(forever ? `<span class="danger-t">Delete ${names(t)} forever?</span>` : `Move ${names(t)} to the Recycle Bin?`,
        `<p class="muted">${forever ? "This skips the Recycle Bin. It cannot be undone." : "You can restore from the Recycle Bin."}</p><div class="mono muted" style="max-height:30vh;overflow:auto">${t.slice(0, 60).map((x) => esc(x.path)).join("<br>")}${t.length > 60 ? "<br>..." : ""}</div>`,
        [["go", forever ? "Delete forever" : "Recycle", forever ? "danger" : "primary"]]);
      if (!ok) return;
      const next = p.rows[Math.min(p.rows.length - 1, p.cursor + 1)]?.name;
      const id = await invoke("files_delete", { paths: t.map((x) => x.path), permanent: forever });
      watchJob(id, () => loadPane(p, p.path, next));
      return;
    }
    case "props": {
      const r = cur(p);
      if (!r || r.up) return;
      modal(loading("Reading properties..."));
      const pr = await guard(() => invoke("files_props", { path: r.path }));
      if (!pr) return closeModal();
      modal(`<h2>${esc(r.name)}</h2><table class="table"><tbody>
        <tr><td class="muted">Location</td><td class="mono wrap">${esc(pr.path)}</td></tr>
        <tr><td class="muted">Size</td><td>${bytes(pr.size)} (${num(pr.size)} bytes)</td></tr>
        ${pr.is_dir ? `<tr><td class="muted">Contains</td><td>${num(pr.files)} files, ${num(pr.dirs)} folders</td></tr>` : ""}
        <tr><td class="muted">Created</td><td>${fmtDate(pr.created)}</td></tr><tr><td class="muted">Modified</td><td>${fmtDate(pr.modified)}</td></tr><tr><td class="muted">Accessed</td><td>${fmtDate(pr.accessed)}</td></tr>
        <tr><td class="muted">Attributes</td><td>${[pr.readonly && "read-only", pr.hidden && "hidden"].filter(Boolean).join(", ") || "-"}</td></tr></tbody></table>
        <div class="row" style="justify-content:flex-end;margin-top:14px"><button class="btn" id="pr-rev">Show in Explorer</button><button class="btn primary" data-close>Close</button></div>`);
      $("#pr-rev").onclick = () => invoke("reveal", { path: pr.path });
      return;
    }
    case "sizes": {
      const dirs = (p.rows.some((r) => p.sel.has(r.path)) ? t : p.rows).filter((r) => r.is_dir && !r.up).map((r) => r.path);
      if (!dirs.length) return;
      setAct("files", `Measuring ${dirs.length} folder${dirs.length > 1 ? "s" : ""}`);
      const res = await guard(() => invoke("files_dir_sizes", { paths: dirs }));
      setAct("files", null);
      (res || []).forEach(([path, size]) => (p.sizes[path] = size));
      if (current === "files") drawPane(p);
      return;
    }
    case "compare": {
      const idx = (x) => new Map(x.rows.filter((r) => !r.up && !r.is_dir).map((r) => [r.name.toLowerCase(), r]));
      const [a, b] = [idx(CMD.panes[0]), idx(CMD.panes[1])];
      let diff = 0;
      for (const [pa, mine, theirs] of [[CMD.panes[0], a, b], [CMD.panes[1], b, a]]) {
        pa.sel.clear();
        for (const [n, r] of mine) {
          const t2 = theirs.get(n);
          if (!t2 || t2.size !== r.size || Math.abs(t2.modified - r.modified) > 2) { pa.sel.add(r.path); diff++; }
        }
        drawPane(pa);
      }
      toast(diff ? `${diff} file${diff > 1 ? "s" : ""} differ or exist on one side only - they are now selected.` : "Both folders have the same files.");
      return;
    }
    case "pack": {
      if (!t.length) return;
      const base = t.length === 1 ? t[0].name.replace(/\.[^.]+$/, "") : (p.path.split(/[\\/]/).filter(Boolean).pop() || "archive");
      const archive = await ask("Pack to zip", "Archive file", joinPath(o.path, `${base}.zip`), "Pack");
      if (!archive) return;
      const id = await invoke("files_pack", { sources: t.map((x) => x.path), archive });
      watchJob(id, () => refreshBoth());
      return;
    }
    case "unpack": {
      const r = cur(p);
      if (!r || r.is_dir || !ARCHIVES.includes(r.ext)) return toast("Put the cursor on a .zip / .tar / .gz archive first.", true);
      const dest = await ask("Unpack", "Extract into folder", joinPath(o.path, r.name.replace(/\.(zip|tar|gz|tgz|bz2|xz|7z)$/gi, "")), "Unpack");
      if (!dest) return;
      const id = await invoke("files_unpack", { archive: r.path, dest });
      watchJob(id, () => refreshBoth());
      return;
    }
    case "search": return searchDialog(p);
    case "mrename": return multiRenameDialog(p);
  }
}

// ------------------------------------------------------------- search ----
let lastSearch = null;
function searchDialog(p) {
  modal(`<h2>Find files</h2>
    <div class="grid g2" style="gap:10px">
      <label class="muted">Name (wildcards ok)<input class="search" id="s-name" style="width:100%;margin-top:4px" placeholder="*.pdf"></label>
      <label class="muted">Containing text (optional)<input class="search" id="s-text" style="width:100%;margin-top:4px"></label></div>
    <label class="muted" style="display:block;margin-top:10px">Search in<input class="search mono" id="s-root" style="width:100%;margin-top:4px" value="${esc(p.path)}"></label>
    <div class="row" style="justify-content:flex-end;margin-top:14px"><button class="btn" data-close>Close</button><button class="btn primary" id="s-go">${icon("search")}Search</button></div>
    <div id="s-res" style="margin-top:12px"></div>`);
  $("#modal-card").classList.add("wide");
  $("#s-name").focus();
  const go = async () => {
    const id = await invoke("files_search", { root: $("#s-root").value, pattern: $("#s-name").value, text: $("#s-text").value });
    lastSearch = id;
    watchJob(id, () => drawResults(), true);
    drawResults();
  };
  $("#s-go").onclick = go;
  $$("#s-name, #s-text").forEach((i) => (i.onkeydown = (e) => e.key === "Enter" && go()));
}
function drawResults() {
  const box = $("#s-res");
  const j = JOBS.list.find((x) => x.id === lastSearch);
  if (!box || !j) return;
  box.innerHTML = `<div class="spread muted" style="margin-bottom:6px"><span>${j.state === "running" ? '<span class="spin sm"></span> searching ' + esc(j.current) : esc(j.message)}</span><span>${j.output.length} found</span></div>
    <div class="scroll mono" style="max-height:40vh">${j.output.slice(0, 1500).map((f) => `<div class="hit" data-hit="${esc(f)}">${esc(f)}</div>`).join("")}</div>`;
  $$("[data-hit]").forEach((h) => (h.onclick = () => {
    const f = h.dataset.hit;
    const cut = Math.max(f.lastIndexOf("\\"), f.lastIndexOf("/"));
    closeModal();
    loadPane(active(), f.slice(0, cut) || f, f.slice(cut + 1));
  }));
}
document.addEventListener("mwm-jobs", () => { if (lastSearch && $("#s-res")) drawResults(); });

// ------------------------------------------------------- multi-rename ----
function multiRenameDialog(p) {
  const files = targets(p).filter((r) => !r.up);
  if (files.length < 1) return toast("Select the files to rename first (Space / Ctrl+A).", true);
  modal(`<h2>Multi-rename ${files.length} item${files.length > 1 ? "s" : ""}</h2>
    <p class="muted" style="margin-top:0">Tokens: <b>[N]</b> name, <b>[E]</b> extension, <b>[C]</b> counter, <b>[D]</b> date (yyyy-mm-dd).</p>
    <div class="grid g3" style="gap:10px">
      <label class="muted">Pattern<input class="search mono" id="mr-pat" style="width:100%;margin-top:4px" value="[N].[E]"></label>
      <label class="muted">Find<input class="search" id="mr-find" style="width:100%;margin-top:4px"></label>
      <label class="muted">Replace with<input class="search" id="mr-rep" style="width:100%;margin-top:4px"></label>
      <label class="muted">Counter starts at<input class="search" id="mr-start" type="number" value="1" style="width:100%;margin-top:4px"></label>
      <label class="muted">Digits<input class="search" id="mr-dig" type="number" value="2" style="width:100%;margin-top:4px"></label>
      <label class="muted">Case<select class="search" id="mr-case" style="width:100%;margin-top:4px"><option value="">unchanged</option><option value="lower">lower</option><option value="upper">UPPER</option><option value="title">Title</option></select></label></div>
    <div class="scroll" style="max-height:38vh;margin-top:12px"><table class="table"><tbody id="mr-prev"></tbody></table></div>
    <div class="row" style="justify-content:flex-end;margin-top:14px"><button class="btn" data-close>Cancel</button><button class="btn primary" id="mr-go">Rename</button></div>`);
  $("#modal-card").classList.add("wide");
  const plan = () => {
    const pat = $("#mr-pat").value, find = $("#mr-find").value, rep = $("#mr-rep").value;
    const start = +$("#mr-start").value || 0, dig = Math.max(1, +$("#mr-dig").value || 1), cs = $("#mr-case").value;
    return files.map((f, i) => {
      const dot = f.is_dir ? -1 : f.name.lastIndexOf(".");
      const base = dot > 0 ? f.name.slice(0, dot) : f.name;
      const ext = dot > 0 ? f.name.slice(dot + 1) : "";
      const d = new Date(f.modified * 1000).toISOString().slice(0, 10);
      let n = pat.replaceAll("[N]", base).replaceAll("[E]", ext).replaceAll("[C]", String(start + i).padStart(dig, "0")).replaceAll("[D]", d);
      if (!ext) n = n.replace(/\.$/, "");
      if (find) n = n.split(find).join(rep);
      if (cs === "lower") n = n.toLowerCase();
      if (cs === "upper") n = n.toUpperCase();
      if (cs === "title") n = n.replace(/\w\S*/g, (w) => w[0].toUpperCase() + w.slice(1).toLowerCase());
      return [f.path, n, f.name];
    });
  };
  const draw = () => { $("#mr-prev").innerHTML = plan().map(([, n, o]) => `<tr><td class="mono muted">${esc(o)}</td><td>→</td><td class="mono ${n !== o ? "accent" : ""}">${esc(n)}</td></tr>`).join(""); };
  $$("#modal-card input, #modal-card select").forEach((i) => (i.oninput = draw));
  draw();
  $("#mr-go").onclick = async () => {
    const pl = plan().filter(([, n, o]) => n !== o).map(([path, n]) => [path, n]);
    if (!pl.length) return closeModal();
    const n = await guard(() => invoke("files_multi_rename", { plan: pl }));
    closeModal();
    if (n !== undefined) { toast(`Renamed ${n} item${n === 1 ? "" : "s"}.`); loadPane(p, p.path, cur(p)?.name); }
  };
}
