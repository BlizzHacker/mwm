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
    totp = page.evaluate("""async () => {
      const now = Date.now; Date.now = () => 59000;
      try { return await vaultTotp('otpauth://totp/test?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&digits=8'); }
      finally { Date.now = now; }
    }""")
    assert totp == "94287082", totp
    assert not errors, errors
    print("KDBX create, save, lock, reopen, and entry round trip passed")
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
