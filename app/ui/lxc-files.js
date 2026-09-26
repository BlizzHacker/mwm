/* Agentless LXC file manager. Commands run on the owning Proxmox host. */
"use strict";

const lxcName = (path) => path.split("/").filter(Boolean).pop() || "/";
const lxcJoin = (dir, name) => `${dir === "/" ? "" : dir}/${name}`;
const lxcBase64 = (bytes) => {
  let text = "";
  for (let i = 0; i < bytes.length; i += 32768) text += String.fromCharCode(...bytes.subarray(i, i + 32768));
  return btoa(text);
};
const lxcBytes = (encoded) => Uint8Array.from(atob(encoded), (char) => char.charCodeAt(0));

async function lxcFilePanel(node, vmid, dir) {
  modal(loading(`Reading LXC ${vmid}: ${dir}`));
  $("#modal-card").classList.add("wide");
  const result = await guard(() => invoke("lxc_fs_list", { node, vmid: String(vmid), dir }));
  if (!result) return;
  const rows = arr(result.entries).sort((a, b) => (b.kind === "d") - (a.kind === "d") || a.name.localeCompare(b.name));
  modal(`<h2>LXC ${esc(vmid)} files <span class="muted" style="font-size:13px">on ${esc(node)}</span></h2>
    <div class="row" style="gap:8px;flex-wrap:wrap;margin-bottom:12px"><button class="btn small" id="lf-up" ${dir === "/" ? "disabled" : ""}>↑ Up</button><input class="search mono" id="lf-path" value="${esc(dir)}" style="flex:1;min-width:180px"><button class="btn small" id="lf-go">Go</button><button class="btn small" id="lf-new">New folder</button><button class="btn small" id="lf-upload">Upload</button><input id="lf-file" type="file" hidden></div>
    <div class="scroll" style="max-height:52vh"><table class="table"><thead><tr><th>Name</th><th>Size</th><th>Modified</th><th>Actions</th></tr></thead><tbody>${rows.map((r, i) => `<tr><td class="cell-main click" data-lf-open="${i}">${r.kind === "d" ? "📁" : "📄"} ${esc(r.name)}</td><td class="num muted">${r.kind === "d" ? "" : bytes(r.size)}</td><td class="muted">${r.modified ? new Date(r.modified * 1000).toLocaleString() : ""}</td><td><button class="btn small" data-lf-download="${i}" ${r.kind === "d" ? "disabled" : ""}>Download</button> <button class="btn small danger" data-lf-delete="${i}">Delete</button></td></tr>`).join("")}</tbody></table></div>
    <p class="muted">Files up to 8 MiB. Access uses Proxmox pct; no software is installed inside this LXC. Folder deletion requires an empty folder.</p><div class="row" style="justify-content:flex-end"><button class="btn" data-close>Close</button></div>`);
  $("#modal-card").classList.add("wide");
  const refresh = () => lxcFilePanel(node, vmid, dir);
  $("#lf-up").onclick = () => lxcFilePanel(node, vmid, dir.replace(/\/[^/]+\/?$/, "") || "/");
  $("#lf-go").onclick = () => lxcFilePanel(node, vmid, $("#lf-path").value.trim());
  $("#lf-path").onkeydown = (e) => { if (e.key === "Enter") $("#lf-go").click(); };
  $$("[data-lf-open]").forEach((el) => el.onclick = () => {
    const row = rows[+el.dataset.lfOpen], path = lxcJoin(dir, row.name);
    if (row.kind === "d") lxcFilePanel(node, vmid, path);
    else lxcEditFile(node, vmid, path);
  });
  $$("[data-lf-download]").forEach((el) => el.onclick = async () => {
    const row = rows[+el.dataset.lfDownload];
    const file = await guard(() => invoke("lxc_fs_read", { node, vmid: String(vmid), path: lxcJoin(dir, row.name) }));
    if (!file) return;
    const url = URL.createObjectURL(new Blob([lxcBytes(file.data)]));
    const link = document.createElement("a"); link.href = url; link.download = row.name; link.click();
    setTimeout(() => URL.revokeObjectURL(url), 10000);
  });
  $$("[data-lf-delete]").forEach((el) => el.onclick = async () => {
    const row = rows[+el.dataset.lfDelete], path = lxcJoin(dir, row.name);
    if (!(await choose(`Delete ${esc(row.name)}?`, `<p class="muted">${esc(path)} inside LXC ${esc(vmid)}. This bypasses the LXC trash; empty folders only.</p>`, [["delete", "Delete", "danger"]]))) return;
    if (await guard(() => invoke("lxc_fs_delete", { node, vmid: String(vmid), path }))) refresh();
  });
  $("#lf-new").onclick = async () => {
    const name = await ask("New folder", "Folder name");
    if (!name || name.includes("/") || name === "." || name === "..") return;
    if (await guard(() => invoke("lxc_fs_mkdir", { node, vmid: String(vmid), path: lxcJoin(dir, name) }))) refresh();
  };
  $("#lf-upload").onclick = () => $("#lf-file").click();
  $("#lf-file").onchange = async (event) => {
    const file = event.target.files[0]; if (!file) return;
    if (file.size > 8 * 1024 * 1024) return toast("LXC upload limit is 8 MiB.", true);
    const path = lxcJoin(dir, file.name);
    if (rows.some((r) => r.name === file.name) && !(await choose("Replace existing file?", `<p>${esc(path)}</p>`, [["replace", "Replace", "danger"]]))) return;
    const data = lxcBase64(new Uint8Array(await file.arrayBuffer()));
    if (await guard(() => invoke("lxc_fs_write", { node, vmid: String(vmid), path, data }))) refresh();
  };
}

async function lxcEditFile(node, vmid, path) {
  const file = await guard(() => invoke("lxc_fs_read", { node, vmid: String(vmid), path }));
  if (!file) return;
  const data = lxcBytes(file.data), text = new TextDecoder("utf-8", { fatal: false }).decode(data);
  if (text.includes("\uFFFD") || text.includes("\0")) return toast("Binary file: use Download and Upload.", true);
  modal(`<h2>Edit ${esc(lxcName(path))}</h2><p class="mono muted wrap">${esc(path)} · LXC ${esc(vmid)} on ${esc(node)}</p><textarea class="search mono" id="lf-text" style="width:100%;height:48vh;resize:vertical">${esc(text)}</textarea><div class="row" style="justify-content:flex-end;margin-top:12px"><button class="btn" data-close>Cancel</button><button class="btn primary" id="lf-save">Save</button></div>`);
  $("#modal-card").classList.add("wide");
  $("#lf-save").onclick = async () => {
    const data = lxcBase64(new TextEncoder().encode($("#lf-text").value));
    if (await guard(() => invoke("lxc_fs_write", { node, vmid: String(vmid), path, data }))) {
      toast("Saved in LXC."); lxcFilePanel(node, vmid, path.replace(/\/[^/]+$/, "") || "/");
    }
  };
}
