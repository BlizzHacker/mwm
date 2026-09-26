/* MWM Vault: KeePass-compatible KDBX3/4. Crypto stays in the browser/WebView. */
"use strict";

const VAULT = { db: null, revision: "missing", blob: null, search: "", group: "", selected: "", pending: null, timer: null, lastUse: 0, scope: "" };
const VK = () => window.kdbxweb;
const vf = (entry, name) => {
  const value = entry.fields.get(name);
  return value instanceof VK().ProtectedValue ? value.getText() : String(value || "");
};
const vb64 = (buffer) => VK().ByteUtils.bytesToBase64(new Uint8Array(buffer));
const vun64 = (str) => {
  const bytes = VK().ByteUtils.base64ToBytes(str);
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
};
function vaultCryptReady() {
  if (!VK() || !window.hashwasm || !crypto?.subtle) throw new Error("KDBX requires a secure browser context and the bundled cryptography library.");
  VK().CryptoEngine.setArgon2Impl(async (password, salt, memory, iterations, length, parallelism, type, version) => {
    if (version !== 0x13) throw new Error("Unsupported Argon2 version in KDBX");
    const fn = type === 0 ? hashwasm.argon2d : type === 2 ? hashwasm.argon2id : null;
    if (!fn) throw new Error("Unsupported Argon2 type in KDBX");
    const output = await fn({ password: new Uint8Array(password), salt: new Uint8Array(salt),
      memorySize: memory, iterations, parallelism, hashLength: length, outputType: "binary" });
    return output.buffer.slice(output.byteOffset, output.byteOffset + output.byteLength);
  });
}
function vaultLock() {
  VAULT.db = null; VAULT.pending = null; VAULT.selected = "";
  if (VAULT.timer) clearInterval(VAULT.timer);
  VAULT.timer = null;
  if (current === "vault") VIEWS.vault();
}
function vaultTouch() { VAULT.lastUse = Date.now(); }
function vaultStartTimer() {
  vaultTouch();
  if (VAULT.timer) clearInterval(VAULT.timer);
  VAULT.timer = setInterval(() => {
    if (VAULT.db && Date.now() - VAULT.lastUse > 10 * 60_000) vaultLock();
  }, 30_000);
}
document.addEventListener("mwm-leave", (e) => { if (e.detail === "vault") vaultLock(); });
document.addEventListener("mwm-modal-closed", () => { if (current === "vault") $("#modal-card").replaceChildren(); });

async function vaultCredentials(pass, keyFile) {
  const key = keyFile ? await keyFile.arrayBuffer() : null;
  return new (VK().Credentials)(pass ? VK().ProtectedValue.fromString(pass) : null, key);
}
async function vaultOpen(buffer, pass, keyFile) {
  vaultCryptReady();
  const db = await VK().Kdbx.load(buffer, await vaultCredentials(pass, keyFile));
  vaultStartTimer();
  return db;
}
async function vaultLoad() {
  const record = await invoke("vault_load");
  VAULT.revision = record.revision;
  VAULT.blob = record.blob;
  VAULT.scope = TARGET;
}
async function vaultStore(buffer) {
  if (VAULT.scope !== TARGET) throw new Error("Machine changed; reopen its vault before saving.");
  const blob = { format: "kdbx", data: vb64(buffer) };
  const record = await invoke("vault_store", { expectedRevision: VAULT.revision, blob });
  VAULT.revision = record.revision;
  VAULT.blob = record.blob;
  VAULT.pending = null;
  vaultTouch();
}
async function vaultSave() {
  if (!VAULT.db) return false;
  try {
    setAct("vault", "Encrypting KDBX database...");
    await vaultStore(await VAULT.db.save());
    toast("Vault saved.");
    vaultDraw();
    return true;
  } catch (e) { toast(String(e?.message || e), true); return false; }
  finally { setAct("vault", null); }
}

VIEWS.vault = async function () {
  if (VAULT.db && VAULT.scope !== TARGET) { VAULT.db = null; VAULT.blob = null; }
  $("#top-actions").innerHTML = VAULT.db
    ? `<button class="btn" id="v-csv">Import browser CSV</button><button class="btn" id="v-keys">Import machine keys</button><button class="btn" id="v-backup">Download KDBX backup</button><button class="btn" id="v-lock">Lock</button><button class="btn primary" id="v-add">New entry</button>`
    : "";
  if (VAULT.db) return vaultDraw();
  $("#page").innerHTML = loading("Checking encrypted vault...");
  try { await vaultLoad(); } catch (e) { $("#page").innerHTML = `<div class="empty">${esc(e?.message || e)}</div>`; return; }
  if (current !== "vault") return;
  $("#page").innerHTML = `<div class="card" style="max-width:660px;margin:auto">
    <h2 style="margin-top:0">${VAULT.blob ? "Unlock your vault" : "Set up your vault"}</h2>
    <p class="muted">MWM uses KeePass-compatible KDBX. Your master password and key file stay in this app. Import your existing .kdbx file without changing the original.</p>
    ${VAULT.blob ? `<label class="label">Master password</label><input class="search" type="password" id="v-pass" autocomplete="off" style="width:100%;margin:7px 0 12px">
      <label class="label">Key file, if used</label><input type="file" id="v-key" style="display:block;margin:7px 0 14px">
      <button class="btn primary" id="v-open">Unlock</button> <button class="btn" id="v-import">Replace with another KDBX file</button>`
      : `<div class="row" style="gap:10px;flex-wrap:wrap"><button class="btn primary" id="v-import">Import KeePassXC KDBX</button><button class="btn" id="v-create">Create new vault</button></div>`}
    <input type="file" id="v-file" accept=".kdbx" hidden><div id="v-msg" class="muted" style="margin-top:12px"></div></div>`;
  $("#v-open") && ($("#v-open").onclick = async () => {
    const pass = $("#v-pass").value, key = $("#v-key").files[0];
    $("#v-msg").textContent = "Decrypting database...";
    try { VAULT.db = await vaultOpen(vun64(VAULT.blob.data), pass, key); $("#v-pass").value = ""; VAULT.search = ""; VIEWS.vault(); }
    catch (e) { $("#v-msg").textContent = `Could not unlock: ${e?.message || e}`; }
  });
  $("#v-pass") && ($("#v-pass").onkeydown = (e) => { if (e.key === "Enter") $("#v-open").click(); });
  $("#v-import").onclick = () => $("#v-file").click();
  $("#v-file").onchange = () => vaultImportPrompt($("#v-file").files[0]);
  $("#v-create") && ($("#v-create").onclick = vaultCreatePrompt);
};

async function vaultImportPrompt(file) {
  if (!file) return;
  if (file.size > 10 * 1024 * 1024) return toast("KDBX database exceeds MWM's 10 MB vault limit.", true);
  modal(`<h2>Import ${esc(file.name)}</h2><p class="muted">The source file stays where it is. Enter its KeePassXC credentials here; MWM decrypts it locally before storing the encrypted KDBX file.</p>
    <label class="label">Master password</label><input type="password" class="search" id="vi-pass" autocomplete="off" style="width:100%;margin:6px 0 12px">
    <label class="label">Key file, if used</label><input type="file" id="vi-key" style="display:block;margin:6px 0 14px">
    <div class="row" style="justify-content:flex-end"><button class="btn" data-close>Cancel</button><button class="btn primary" id="vi-go">Open file</button></div><div id="vi-error" class="muted"></div>`);
  $("#vi-go").onclick = async () => {
    const pass = $("#vi-pass").value, key = $("#vi-key").files[0];
    $("#vi-error").textContent = "Opening encrypted file...";
    try {
      const raw = await file.arrayBuffer();
      const db = await vaultOpen(raw, pass, key);
      $("#vi-pass").value = "";
      closeModal();
      const count = [...db.getDefaultGroup().allEntries()].length;
      const decision = await choose("Use this KDBX as the MWM vault?",
        `<p class="muted">${count} entries opened successfully. MWM will save the encrypted database on the selected machine. ${VAULT.blob ? "This replaces its current MWM vault; download a backup first if needed." : "Your original KeePassXC file is untouched."}</p>`, [["save", "Store encrypted KDBX", "primary"]]);
      if (decision !== "save") return;
      await vaultStore(raw);
      VAULT.db = db; VAULT.search = ""; VAULT.group = ""; VAULT.selected = "";
      VIEWS.vault(); toast(`Imported ${count} entries.`);
    } catch (e) { const box = $("#vi-error"); if (box) box.textContent = `Import failed: ${e?.message || e}`; else toast(String(e?.message || e), true); }
  };
}

async function vaultCreatePrompt() {
  modal(`<h2>Create KDBX vault</h2><p class="muted">Choose a master password you can remember. MWM cannot recover it.</p>
    <label class="label">Master password</label><input class="search" id="vc-pass" type="password" autocomplete="new-password" style="width:100%;margin:6px 0 10px">
    <label class="label">Confirm password</label><input class="search" id="vc-confirm" type="password" autocomplete="new-password" style="width:100%;margin:6px 0 12px">
    <div class="row" style="justify-content:flex-end"><button class="btn" data-close>Cancel</button><button class="btn primary" id="vc-go">Create</button></div><div id="vc-error" class="muted"></div>`);
  $("#vc-go").onclick = async () => {
    const pass = $("#vc-pass").value;
    if (pass.length < 12 || pass !== $("#vc-confirm").value) return $("#vc-error").textContent = "Use at least 12 characters, and match both fields.";
    try {
      vaultCryptReady();
      const db = VK().Kdbx.create(await vaultCredentials(pass, null), "MWM Vault");
      $("#vc-pass").value = $("#vc-confirm").value = "";
      await vaultStore(await db.save());
      closeModal(); VAULT.db = db; vaultStartTimer(); VIEWS.vault();
    } catch (e) { $("#vc-error").textContent = String(e?.message || e); }
  };
}

function vaultGroups() {
  if (!VAULT.db) return [];
  const root = VAULT.db.getDefaultGroup();
  return [root, ...root.allGroups()].filter((g) => !g.uuid.equals?.(VAULT.db.meta.recycleBinUuid));
}
function vaultFind(id) {
  for (const group of vaultGroups()) for (const entry of group.entries) if (entry.uuid.id === id) return entry;
  return null;
}
function vaultDraw() {
  if (!VAULT.db || current !== "vault") return;
  if (VAULT.scope !== TARGET) return vaultLock();
  vaultTouch();
  const groups = vaultGroups();
  const all = groups.flatMap((g) => g.entries);
  const q = VAULT.search.toLowerCase();
  const entries = all.filter((e) => (!VAULT.group || e.parentGroup?.uuid.id === VAULT.group) &&
    (!q || ["Title", "UserName", "URL", "Notes"].some((f) => vf(e, f).toLowerCase().includes(q))));
  const selected = VAULT.selected ? vaultFind(VAULT.selected) : null;
  $("#page").innerHTML = `<div class="row" style="gap:10px;flex-wrap:wrap;margin-bottom:14px">
      <input class="search" id="v-search" placeholder="Search entries" value="${esc(VAULT.search)}" style="flex:1;min-width:180px">
      <select class="search" id="v-group" style="min-width:150px"><option value="">All groups</option>${groups.map((g) => `<option value="${esc(g.uuid.id)}" ${VAULT.group === g.uuid.id ? "selected" : ""}>${esc(g.name)}</option>`).join("")}</select>
      <button class="btn" id="v-newgroup">New group</button></div>
    <div class="grid g2" style="grid-template-columns:minmax(230px,40%) minmax(300px,1fr);gap:12px">
      <div class="card" style="padding:0;max-height:68vh;overflow:auto">${entries.length ? entries.map((e) => `<div class="row" data-v-entry="${esc(e.uuid.id)}" style="cursor:pointer;padding:10px 14px;border-bottom:1px solid var(--line);background:${selected === e ? "var(--panel2,#19263a)" : ""}"><div style="min-width:0"><b>${esc(vf(e,"Title") || "Untitled")}</b><div class="cell-sub">${esc(vf(e,"UserName"))} · ${esc(e.parentGroup?.name || "")}</div></div></div>`).join("") : `<div class="empty">No entries found.</div>`}</div>
      <div class="card" id="v-detail">${selected ? vaultDetail(selected) : `<div class="empty">Select an entry to view its details.</div>`}</div></div>`;
  $("#v-search").oninput = (e) => { VAULT.search = e.target.value; vaultDraw(); $("#v-search")?.focus(); $("#v-search")?.setSelectionRange(VAULT.search.length, VAULT.search.length); };
  $("#v-group").onchange = (e) => { VAULT.group = e.target.value; VAULT.selected = ""; vaultDraw(); };
  $("#v-newgroup").onclick = async () => { const name = await ask("New group", "Group name"); if (name?.trim()) { VAULT.db.createGroup(VAULT.db.getDefaultGroup(), name.trim()); vaultSave(); } };
  $$("[data-v-entry]").forEach((el) => (el.onclick = () => { VAULT.selected = el.dataset.vEntry; vaultDraw(); }));
  $("#v-add").onclick = () => vaultEdit(null);
  $("#v-csv").onclick = vaultImportBrowserPrompt;
  $("#v-keys").onclick = vaultImportMachineKeys;
  $("#v-lock").onclick = vaultLock;
  $("#v-backup").onclick = vaultBackup;
  if (selected) vaultBindDetail(selected);
}
function vaultParseCsv(source) {
  const rows = [], row = [];
  let field = "", quoted = false, closedQuote = false;
  const input = source.replace(/^\uFEFF/, "");
  for (let i = 0; i < input.length; i++) {
    const ch = input[i];
    if (quoted) {
      if (ch === '"' && input[i + 1] === '"') { field += '"'; i++; }
      else if (ch === '"') { quoted = false; closedQuote = true; }
      else field += ch;
    } else if (ch === ',' || ch === '\n' || ch === '\r') {
      row.push(field); field = ""; closedQuote = false;
      if (ch !== ',') {
        if (ch === '\r' && input[i + 1] === '\n') i++;
        if (row.some((cell) => cell !== "")) rows.push(row.slice());
        row.length = 0;
      }
    } else if (ch === '"' && !field && !closedQuote) quoted = true;
    else if (closedQuote && ch !== ' ' && ch !== '\t') throw new Error("Invalid CSV: text follows a closing quote.");
    else if (!closedQuote) field += ch;
  }
  if (quoted) throw new Error("Invalid CSV: an entry has an unclosed quote.");
  row.push(field);
  if (row.some((cell) => cell !== "")) rows.push(row);
  if (rows.length < 2) throw new Error("CSV has no password entries.");
  const header = rows.shift().map((cell) => cell.trim().toLowerCase().replace(/[\s_-]+/g, ""));
  const pick = (...names) => header.findIndex((cell) => names.includes(cell));
  const column = { name: pick("name", "title"), url: pick("url", "website", "origin", "loginuri"),
    user: pick("username", "user", "login"), pass: pick("password"), notes: pick("note", "notes") };
  if (column.pass < 0 || (column.url < 0 && column.name < 0) || column.user < 0)
    throw new Error("Expected browser CSV columns: username, password, and name or url.");
  if (rows.some((cells) => cells.length !== header.length)) throw new Error("CSV has a row with the wrong number of columns.");
  const at = (cells, index) => index < 0 ? "" : cells[index];
  return rows.map((cells) => ({ title: at(cells, column.name), url: at(cells, column.url),
    user: at(cells, column.user), password: at(cells, column.pass), notes: at(cells, column.notes) }))
    .filter((item) => item.password && (item.title || item.url));
}
async function vaultImportBrowserPrompt() {
  if (!VAULT.db || VAULT.scope !== TARGET) return;
  modal(`<h2>Import browser passwords</h2><p class="muted">Export passwords to CSV from Google Password Manager, Brave, or Microsoft Edge, then select that file here. MWM reads it locally and stores the entries in your encrypted KDBX vault. The CSV is plaintext; delete your export after checking the import.</p>
    <label class="label">Source</label><select class="search" id="vci-source" style="width:100%;margin:6px 0 12px"><option value="Google">Google Password Manager</option><option value="Brave">Brave</option><option value="Edge">Microsoft Edge / Windows browser</option></select>
    <label class="label">Exported CSV</label><input type="file" id="vci-file" accept=".csv,text/csv" style="display:block;margin:6px 0 14px">
    <div class="row" style="justify-content:flex-end"><button class="btn" data-close>Cancel</button><button class="btn primary" id="vci-go">Review import</button></div><div id="vci-error" class="muted"></div>`);
  $("#vci-go").onclick = async () => {
    const file = $("#vci-file").files[0], source = $("#vci-source").value;
    if (!file) return $("#vci-error").textContent = "Select an exported CSV file.";
    if (file.size > 5 * 1024 * 1024) return $("#vci-error").textContent = "CSV files are limited to 5 MB.";
    try {
      const entries = vaultParseCsv(await file.text());
      if (!entries.length) throw new Error("No entries with passwords were found.");
      closeModal();
      const choice = await choose("Import browser passwords?", `<p class="muted">${entries.length} entries found in ${esc(file.name)}. Matching entries in the ${source} group will be updated with KDBX history. The plaintext CSV is never uploaded to the MWM agent.</p>`, [["go", "Import into vault", "primary"]]);
      if (choice !== "go") return;
      if (!VAULT.db || VAULT.scope !== TARGET) throw new Error("Vault locked or machine changed. Reopen it and retry.");
      let group = vaultGroups().find((g) => g.name === `Browser passwords - ${source}`);
      if (!group) group = VAULT.db.createGroup(VAULT.db.getDefaultGroup(), `Browser passwords - ${source}`);
      let added = 0, updated = 0, unchanged = 0;
      for (const item of entries) {
        const title = item.title || item.url;
        let entry = group.entries.find((e) => vf(e, "URL") === item.url && vf(e, "UserName") === item.user &&
          (item.url || vf(e, "Title") === title));
        if (entry && vf(entry, "Title") === title && vf(entry, "Password") === item.password && vf(entry, "Notes") === item.notes) { unchanged++; continue; }
        if (entry) { entry.pushHistory(); updated++; }
        else { entry = VAULT.db.createEntry(group); added++; }
        entry.fields.set("Title", title);
        entry.fields.set("URL", item.url);
        entry.fields.set("UserName", item.user);
        entry.fields.set("Password", VK().ProtectedValue.fromString(item.password));
        entry.fields.set("Notes", item.notes);
        entry.times.update();
      }
      if (!await vaultSave()) throw new Error("Import is in memory but could not be saved. Keep the CSV and retry saving the vault.");
      toast(`Browser passwords: ${added} added, ${updated} updated, ${unchanged} unchanged. Delete the plaintext CSV after checking.`);
    } catch (e) { const box = $("#vci-error"); if (box) box.textContent = String(e?.message || e); else toast(String(e?.message || e), true); }
  };
}
async function vaultImportMachineKeys() {
  if (!VAULT.db || VAULT.scope !== TARGET) return;
  const items = await guard(() => invoke("keys_list"));
  if (!items) return;
  const found = items.filter((item) => item.value);
  if (!found.length) return toast("No readable machine keys were found.");
  const choice = await choose("Import machine keys?", `<p class="muted">Add ${found.length} readable licenses, recovery keys and network secrets from the selected machine to this encrypted KDBX. Existing entries with the same title will be updated.</p>`, [["go", "Import to vault", "primary"]]);
  if (choice !== "go") return;
  const host = S.info?.hostname || "Machine";
  let group = vaultGroups().find((g) => g.name === `Machine keys - ${host}`);
  if (!group) group = VAULT.db.createGroup(VAULT.db.getDefaultGroup(), `Machine keys - ${host}`);
  for (const item of found) {
    const title = `${item.name} (${item.kind})`;
    let entry = group.entries.find((e) => vf(e,"Title") === title);
    if (entry) entry.pushHistory(); else entry = VAULT.db.createEntry(group);
    entry.fields.set("Title", title);
    entry.fields.set("Password", VK().ProtectedValue.fromString(item.value));
    entry.fields.set("Notes", [item.source, item.note].filter(Boolean).join("\n"));
    entry.times.update();
  }
  await vaultSave();
  toast(`Imported ${found.length} machine keys.`);
}
function vaultDetail(entry) {
  const url = vf(entry,"URL");
  return `<h2 style="margin-top:0">${esc(vf(entry,"Title") || "Untitled")}</h2>
    <div class="label">Username</div><div class="row" style="margin:5px 0 12px"><span class="mono keyval" style="flex:1">${esc(vf(entry,"UserName"))}</span><button class="btn small" id="v-copyuser">Copy</button></div>
    <div class="label">Password</div><div class="row" style="margin:5px 0 12px"><span class="mono keyval" id="v-password" style="flex:1">••••••••••••</span><button class="btn small" id="v-reveal">Show</button><button class="btn small" id="v-copypass">Copy</button></div>
    <div class="label">URL</div><div style="margin:5px 0 12px;overflow-wrap:anywhere">${url ? `<a id="v-url" href="#">${esc(url)}</a>` : "—"}</div>
    <div class="label">Notes</div><div style="white-space:pre-wrap;margin:5px 0 12px">${esc(vf(entry,"Notes")) || "—"}</div>
    ${entry.fields.has("otp") ? `<div class="label">One-time code</div><div class="row"><span class="mono keyval" id="v-totp">••••••</span><button class="btn small" id="v-totpgo">Show code</button></div>` : ""}
    ${entry.binaries.size ? `<div class="label" style="margin-top:15px">Attachments</div>${[...entry.binaries.keys()].map((n) => `<button class="btn small" data-v-attachment="${esc(n)}" style="margin:5px">${esc(n)}</button>`).join("")}` : ""}
    <div class="cell-sub" style="margin:14px 0">${entry.history.length} earlier version(s)</div>
    <div class="row"><button class="btn primary" id="v-edit">Edit</button><button class="btn danger" id="v-delete">Delete</button></div>`;
}
function vaultBindDetail(entry) {
  $("#v-copyuser").onclick = () => copyText(vf(entry,"UserName"));
  $("#v-copypass").onclick = () => copyText(vf(entry,"Password"));
  $("#v-reveal").onclick = () => {
    const box = $("#v-password"), show = box.textContent === "••••••••••••";
    box.textContent = show ? vf(entry,"Password") : "••••••••••••";
    $("#v-reveal").textContent = show ? "Hide" : "Show";
  };
  $("#v-url") && ($("#v-url").onclick = (e) => { e.preventDefault(); const url = vf(entry,"URL"); if (/^https?:\/\//i.test(url)) window.open(url, "_blank", "noopener,noreferrer"); });
  $("#v-edit").onclick = () => vaultEdit(entry);
  $("#v-delete").onclick = async () => {
    if (await choose("Delete entry?", `<p class="muted">Move ${esc(vf(entry,"Title"))} to the KDBX recycle bin?</p>`, [["go","Move to recycle bin","danger"]])) {
      VAULT.db.remove(entry); VAULT.selected = ""; await vaultSave();
    }
  };
  $("#v-totpgo") && ($("#v-totpgo").onclick = async () => {
    try { $("#v-totp").textContent = await vaultTotp(vf(entry,"otp")); }
    catch (e) { toast(String(e?.message || e), true); }
  });
  $$("[data-v-attachment]").forEach((button) => (button.onclick = () => vaultAttachment(entry, button.dataset.vAttachment)));
}
function vaultGenerate(length = 24) {
  const chars = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!@#$%^&*-_=+";
  let out = "";
  while (out.length < length) {
    const random = crypto.getRandomValues(new Uint8Array(64));
    for (const n of random) if (n < Math.floor(256 / chars.length) * chars.length && out.length < length) out += chars[n % chars.length];
  }
  return out;
}
function vaultEdit(entry) {
  const isNew = !entry;
  const groups = vaultGroups();
  modal(`<h2>${isNew ? "New entry" : "Edit entry"}</h2>
    ${[["Title","text"],["UserName","text"],["Password","password"],["URL","url"],["Notes","text"],["otp","text"]].map(([field,type]) =>
      `<label class="label" style="display:block;margin-top:10px">${field === "otp" ? "TOTP secret / otpauth URL" : field}</label><input class="search" id="ve-${field}" type="${type}" value="${esc(entry ? vf(entry,field) : "")}" autocomplete="off" style="width:100%;margin-top:4px">`).join("")}
    <div class="row" style="margin-top:10px"><button class="btn small" id="ve-generate">Generate password</button><button class="btn small" id="ve-show">Show password</button></div>
    <label class="label" style="display:block;margin-top:10px">Group</label><select class="search" id="ve-group" style="width:100%;margin-top:4px">${groups.map((g) => `<option value="${esc(g.uuid.id)}" ${(entry?.parentGroup?.uuid.id || VAULT.group || groups[0]?.uuid.id) === g.uuid.id ? "selected" : ""}>${esc(g.name)}</option>`).join("")}</select>
    <label class="label" style="display:block;margin-top:10px">Add attachment</label><input type="file" id="ve-file" style="display:block;margin-top:4px">
    <div class="row" style="justify-content:flex-end;margin-top:18px"><button class="btn" data-close>Cancel</button><button class="btn primary" id="ve-save">Save</button></div><div id="ve-error" class="muted"></div>`);
  $("#modal-card").classList.add("wide");
  $("#ve-generate").onclick = () => { $("#ve-Password").value = vaultGenerate(); $("#ve-Password").type = "text"; };
  $("#ve-show").onclick = () => { const el = $("#ve-Password"); el.type = el.type === "password" ? "text" : "password"; };
  $("#ve-save").onclick = async () => {
    if (!$("#ve-Title").value.trim()) return $("#ve-error").textContent = "Give the entry a title.";
    try {
      const group = groups.find((g) => g.uuid.id === $("#ve-group").value) || groups[0];
      const target = entry || VAULT.db.createEntry(group);
      if (entry) { entry.pushHistory(); if (entry.parentGroup !== group) VAULT.db.move(entry, group); }
      for (const field of ["Title","UserName","Password","URL","Notes","otp"]) {
        const value = $(`#ve-${field}`).value;
        target.fields.set(field, field === "Password" ? VK().ProtectedValue.fromString(value) : value);
      }
      const file = $("#ve-file").files[0];
      if (file) {
        if (file.size > 5 * 1024 * 1024) throw new Error("Attachments are limited to 5 MB each.");
        const binary = await VAULT.db.binaries.add(await file.arrayBuffer());
        target.binaries.set(file.name, binary);
      }
      target.times.update();
      VAULT.selected = target.uuid.id;
      closeModal(); await vaultSave();
    } catch (e) { $("#ve-error") && ($("#ve-error").textContent = String(e?.message || e)); }
  };
}

function vaultDownload(name, bytes, mime) {
  const url = URL.createObjectURL(new Blob([bytes], { type: mime }));
  const a = document.createElement("a"); a.href = url; a.download = name; a.click();
  setTimeout(() => URL.revokeObjectURL(url), 5000);
}
function vaultBackup() {
  if (!VAULT.blob) return;
  vaultDownload(`MWM-Vault-${new Date().toISOString().slice(0,10)}.kdbx`, vun64(VAULT.blob.data), "application/octet-stream");
}
function vaultAttachment(entry, name) {
  const ref = entry.binaries.get(name);
  const value = ref?.value || (ref?.ref ? VAULT.db.binaries.getByRef(ref)?.value : ref);
  if (!value) return toast("Attachment unavailable.", true);
  const bytes = value instanceof VK().ProtectedValue ? value.getBinary() : value;
  vaultDownload(name, bytes, "application/octet-stream");
}
async function vaultTotp(raw) {
  const input = raw.trim();
  const params = input.startsWith("otpauth://") ? new URL(input).searchParams : new URLSearchParams(input.includes("=") ? input : `secret=${input}`);
  const secret = (params.get("secret") || "").toUpperCase().replace(/[\s=]/g, "");
  const digits = Math.min(8, Math.max(6, Number(params.get("digits") || 6)));
  const period = Math.max(1, Number(params.get("period") || 30));
  const algorithm = (params.get("algorithm") || "SHA1").toUpperCase().replace("SHA", "SHA-");
  if (!/^[A-Z2-7]+$/.test(secret) || !["SHA-1","SHA-256","SHA-512"].includes(algorithm)) throw new Error("Unsupported TOTP secret or algorithm.");
  let bits = 0, count = 0, bytes = [];
  for (const c of secret) { bits = (bits << 5) | "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567".indexOf(c); count += 5; if (count >= 8) { count -= 8; bytes.push((bits >>> count) & 255); } }
  const key = await crypto.subtle.importKey("raw", new Uint8Array(bytes), { name: "HMAC", hash: algorithm }, false, ["sign"]);
  const counter = new ArrayBuffer(8); const view = new DataView(counter); const n = Math.floor(Date.now() / 1000 / period);
  view.setUint32(0, Math.floor(n / 0x100000000)); view.setUint32(4, n >>> 0);
  const mac = new Uint8Array(await crypto.subtle.sign("HMAC", key, counter));
  const pos = mac[mac.length - 1] & 15;
  return String((((mac[pos] & 127) << 24) | (mac[pos+1] << 16) | (mac[pos+2] << 8) | mac[pos+3]) % (10 ** digits)).padStart(digits, "0");
}
