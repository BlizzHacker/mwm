"""Browser KDBX and MWM Vault smoke test with synthetic credentials only."""
from playwright.sync_api import sync_playwright
import base64
import os
import subprocess
import tempfile

with sync_playwright() as p:
    browser = p.chromium.launch(headless=True)
    page = browser.new_page()
    errors = []
    page.on("pageerror", lambda err: errors.append(str(err)))
    page.goto("http://127.0.0.1:8765/index.html")
    page.evaluate("""() => {
      window.__vaultRecord = { revision: 'missing', blob: null };
      window.MWM_DEMO = async (cmd, args) => {
        if (cmd === 'vault_load') return window.__vaultRecord;
        if (cmd === 'vault_store') {
          if (args.expectedRevision !== window.__vaultRecord.revision) throw Error('revision mismatch');
          window.__vaultRecord = { revision: String(Date.now()), blob: args.blob };
          return window.__vaultRecord;
        }
        return {};
      };
      go('vault');
    }""")
    page.get_by_role("button", name="Create new vault").click()
    page.locator("#vc-pass").fill("Synthetic smoke password 456")
    page.locator("#vc-confirm").fill("Synthetic smoke password 456")
    page.locator("#vc-go").click()
    page.get_by_role("button", name="New entry").wait_for(timeout=30000)
    page.get_by_role("button", name="New entry").click()
    page.locator("#ve-Title").fill("Example login")
    page.locator("#ve-UserName").fill("synthetic-user")
    page.locator("#ve-Password").fill("synthetic-secret")
    page.locator("#ve-save").click()
    page.get_by_text("Example login").first.wait_for(timeout=30000)
    page.get_by_role("button", name="Lock").click()
    page.locator("#v-pass").fill("Synthetic smoke password 456")
    page.get_by_role("button", name="Unlock").click()
    page.get_by_text("Example login").first.wait_for(timeout=30000)
    page.get_by_text("Example login").first.click()
    page.locator("#v-reveal").click()
    assert page.locator("#v-password").inner_text() == "synthetic-secret"
    csv = '\ufeffname,url,username,password,note\r\n"Example, site",https://example.test/login,synthetic-user,"synthetic, secret","first line\nsecond line"\r\n'
    page.get_by_role("button", name="Import browser CSV").click()
    page.locator("#vci-source").select_option("Brave")
    page.locator("#vci-file").set_input_files({"name": "synthetic.csv", "mimeType": "text/csv", "buffer": csv.encode("utf-8")})
    page.locator("#vci-go").click()
    page.locator('[data-ch="go"]').click()
    page.get_by_text("Example, site").first.wait_for(timeout=30000)
    page.get_by_text("Example, site").first.click()
    assert page.locator("#v-password").inner_text() == "••••••••••••"
    page.locator("#v-reveal").click()
    assert page.locator("#v-password").inner_text() == "synthetic, secret"
    assert "first line\nsecond line" in page.locator("#v-detail").inner_text()
    assert page.evaluate("!!VAULT.db.getDefaultGroup().groups.find(g => g.name === 'Browser passwords - Brave')")
    audit = page.evaluate("""() => {
      const root = VAULT.db.getDefaultGroup();
      const brave = root.groups.find(g => g.name === 'Browser passwords - Brave');
      const edge = VAULT.db.createGroup(root, 'Browser passwords - Edge');
      const google = VAULT.db.createGroup(root, 'Browser passwords - Google');
      const original = brave.entries.find(e => e.fields.get('Title') === 'Example, site');
      const copy = VAULT.db.createEntry(edge);
      for (const [key, value] of original.fields) copy.fields.set(key, value);
      const conflict = VAULT.db.createEntry(google);
      for (const [key, value] of original.fields) conflict.fields.set(key, value);
      conflict.fields.set('Password', kdbxweb.ProtectedValue.fromString('different synthetic secret'));
      const review = vaultAudit();
      const removable = review.removable, conflicting = review.conflicts.length;
      for (const copies of review.exactGroups) for (const entry of copies.slice(1)) VAULT.db.remove(entry);
      return { removable, conflicting, remaining: vaultAudit().removable };
    }""")
    assert audit == {"removable": 1, "conflicting": 1, "remaining": 0}, audit
    parsed = page.evaluate("vaultParseCsv('name,url,username,password\\nA,https://x.test,u,p\\n')")
    assert len(parsed) == 1 and parsed[0]["password"] == "p"
    page.get_by_role("button", name="Lock").click()
    page.locator("#v-pass").fill("Synthetic smoke password 456")
    page.get_by_role("button", name="Unlock").click()
    page.get_by_text("Example, site").first.wait_for(timeout=30000)
    totp = page.evaluate("""async () => {
      const now = Date.now; Date.now = () => 59000;
      try { return await vaultTotp('otpauth://totp/test?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&digits=8'); }
      finally { Date.now = now; }
    }""")
    assert totp == "94287082", totp
    assert not errors, errors
    print("KDBX create, browser CSV import, duplicate review, lock, reopen, and entry round trip passed")
    cli = r"C:\Program Files\KeePassXC\keepassxc-cli.exe"
    if os.path.exists(cli):
        with tempfile.TemporaryDirectory(prefix="mwm-kdbx-smoke-") as work:
            original = os.path.join(work, "original.kdbx")
            edited = os.path.join(work, "edited.kdbx")
            secret = "Synthetic smoke password 456"
            created = subprocess.run([cli, "db-create", "-p", original], input=f"{secret}\n{secret}\n", text=True, capture_output=True, timeout=30)
            assert created.returncode == 0, created.stderr
            encoded = base64.b64encode(open(original, "rb").read()).decode()
            saved = page.evaluate("""async ({data, password}) => {
              const db = await vaultOpen(vun64(data), password, null);
              const entry = db.createEntry(db.getDefaultGroup());
              entry.fields.set('Title', 'KeePassXC interop entry');
              entry.fields.set('Password', kdbxweb.ProtectedValue.fromString('synthetic'));
              return vb64(await db.save());
            }""", {"data": encoded, "password": secret})
            with open(edited, "wb") as out:
                out.write(base64.b64decode(saved))
            listing = subprocess.run([cli, "ls", edited], input=f"{secret}\n", text=True, capture_output=True, timeout=30)
            assert listing.returncode == 0 and "KeePassXC interop entry" in listing.stdout, (listing.stdout, listing.stderr)
            print("KeePassXC-created Argon2 KDBX opened in MWM and saved back to KeePassXC")
    browser.close()
