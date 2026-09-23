//! Keys vault: product keys, licenses, recovery keys and saved secrets that
//! belong to *this* machine - the things a tech pulls before a reinstall.

use serde::Serialize;
#[allow(unused_imports)]
use serde_json::Value;

#[allow(unused_imports)]
use crate::util;

#[derive(Debug, Clone, Serialize)]
pub struct KeyItem {
    /// windows | office | license | bitlocker | wifi | ssh | proxmox | unraid | wireguard
    pub kind: String,
    pub name: String,
    pub value: String,
    /// Where it came from, e.g. "BIOS (OA3)", "Registry (DigitalProductId)".
    pub source: String,
    pub note: String,
    pub secret: bool,
}

fn item(kind: &str, name: &str, value: &str, source: &str, note: &str, secret: bool) -> KeyItem {
    KeyItem { kind: kind.into(), name: name.into(), value: value.into(), source: source.into(), note: note.into(), secret }
}

/// Decode a DigitalProductId (Windows 7-11, Office 2010-2013) into a
/// 25-character product key. Offset 52 holds the 15 encoded bytes.
pub fn decode_product_key(dpid: &[u8]) -> Option<String> {
    const OFFSET: usize = 52;
    const CHARS: &[u8; 24] = b"BCDFGHJKMPQRTVWXY2346789";
    if dpid.len() < OFFSET + 15 {
        return None;
    }
    let mut k: Vec<u8> = dpid[OFFSET..OFFSET + 15].to_vec();
    let win8 = (k[14] / 6) & 1;
    k[14] = (k[14] & 0xF7) | ((win8 & 2) * 4);
    let mut out = [0u8; 25];
    let mut last = 0usize;
    for i in (0..25).rev() {
        let mut cur: u32 = 0;
        for j in (0..15).rev() {
            cur = (cur << 8) | k[j] as u32;
            k[j] = (cur / 24) as u8;
            cur %= 24;
        }
        out[i] = CHARS[cur as usize];
        last = cur as usize;
    }
    let mut s: Vec<u8> = out.to_vec();
    if win8 == 1 {
        let mut t: Vec<u8> = s[1..].to_vec();
        t.insert(last.min(t.len()), b'N');
        s = t;
    }
    let s = String::from_utf8(s).ok()?;
    if s.chars().all(|c| c == 'B') {
        return None; // all-zero payload
    }
    Some(s.as_bytes().chunks(5).map(|c| String::from_utf8_lossy(c).to_string()).collect::<Vec<_>>().join("-"))
}

#[cfg(windows)]
pub fn list() -> Vec<KeyItem> {
    use winreg::enums::*;
    use winreg::RegKey;
    let mut v = Vec::new();

    // 1. Licensing service: OEM key in firmware + every licensed product.
    let script = r#"
$svc = Get-CimInstance SoftwareLicensingService
$prods = @(Get-CimInstance SoftwareLicensingProduct -Filter "PartialProductKey IS NOT NULL" | ForEach-Object { [pscustomobject]@{Name=$_.Name; Desc=$_.Description; Partial=$_.PartialProductKey; Status=$_.LicenseStatus; Channel=$_.ProductKeyChannel} })
[pscustomobject]@{ Oem=$svc.OA3xOriginalProductKey; OemDesc=$svc.OA3xOriginalProductKeyDescription; Products=$prods } | ConvertTo-Json -Depth 3 -Compress"#;
    let lic = util::powershell_json(script).ok().and_then(|x| x.into_iter().next()).unwrap_or(Value::Null);
    let oem = util::js_str(&lic, "Oem");
    if !oem.is_empty() {
        v.push(item("windows", "Windows product key (firmware)", &oem, "BIOS / UEFI (OA3)", &util::js_str(&lic, "OemDesc"), true));
    }

    // 2. Installed key decoded from the registry (ProduKey method).
    let nt = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", KEY_READ | KEY_WOW64_64KEY);
    if let Ok(nt) = nt {
        let edition: String = nt.get_value("ProductName").unwrap_or_default();
        let pid: String = nt.get_value("ProductId").unwrap_or_default();
        if let Ok(raw) = nt.get_raw_value("DigitalProductId") {
            if let Some(k) = decode_product_key(&raw.bytes) {
                let generic = k.starts_with("YTMG3") || k.starts_with("VK7JG") || k.starts_with("BT79Q");
                v.push(item(
                    "windows",
                    &format!("Installed key ({})", edition.trim()),
                    &k,
                    "Registry (DigitalProductId)",
                    if generic || oem.is_empty() && k.starts_with("YTMG") { "Generic key - this PC is activated by a digital license tied to its hardware / Microsoft account." } else { "" },
                    true,
                ));
            }
        }
        if !pid.is_empty() {
            v.push(item("windows", "Windows Product ID", &pid, "Registry", "Not a key - identifies this installation.", false));
        }
    }
    for p in lic.get("Products").and_then(|p| p.as_array()).cloned().unwrap_or_default() {
        let status = match p.get("Status").and_then(|s| s.as_i64()) {
            Some(1) => "Licensed",
            Some(0) => "Unlicensed",
            Some(5) => "Notification",
            _ => "Grace / other",
        };
        let name = util::js_str(&p, "Name");
        let kind = if name.to_lowercase().contains("office") { "office" } else { "license" };
        v.push(item(kind, &name, &format!("*****-*****-*****-*****-{}", util::js_str(&p, "Partial")), &util::js_str(&p, "Desc"), &format!("{status}. Only the last 5 characters are stored for this product."), false));
    }

    // 3. Older Office (2010/2013) keys decodable from the registry.
    for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
        for ver in ["14.0", "15.0"] {
            let path = format!(r"SOFTWARE\Microsoft\Office\{ver}\Registration");
            let Ok(reg) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(&path, KEY_READ | view) else { continue };
            for sub in reg.enum_keys().flatten() {
                let Ok(k) = reg.open_subkey_with_flags(&sub, KEY_READ) else { continue };
                if let (Ok(raw), name) = (k.get_raw_value("DigitalProductId"), k.get_value::<String, _>("ProductName").unwrap_or_else(|_| format!("Office {ver}"))) {
                    if let Some(key) = decode_product_key(&raw.bytes) {
                        v.push(item("office", &name, &key, "Registry (Office DigitalProductId)", "", true));
                    }
                }
            }
        }
    }

    // 4. BitLocker recovery passwords (administrator only).
    let bl = util::powershell_json(r#"Get-BitLockerVolume -ErrorAction Stop | ForEach-Object { $mp=$_.MountPoint; $_.KeyProtector | Where-Object { $_.KeyProtectorType -eq 'RecoveryPassword' } | ForEach-Object { [pscustomobject]@{Drive=$mp; Id="$($_.KeyProtectorId)"; Key=$_.RecoveryPassword} } } | ConvertTo-Json -Compress"#);
    match bl {
        Ok(rows) if !rows.is_empty() => {
            for r in rows {
                v.push(item("bitlocker", &format!("BitLocker recovery key {}", util::js_str(&r, "Drive")), &util::js_str(&r, "Key"), &format!("Protector {}", util::js_str(&r, "Id")), "Keep this somewhere safe - it unlocks the drive if Windows can't.", true));
            }
        }
        _ => {
            if !crate::sys::is_elevated() {
                v.push(item("bitlocker", "BitLocker recovery keys", "", "manage-bde", "Restart MWM as administrator to read recovery keys.", false));
            }
        }
    }

    // 5. Wi-Fi.
    for w in crate::toolkit::wifi().as_array().cloned().unwrap_or_default() {
        let key = util::js_str(&w, "Key");
        v.push(item("wifi", &util::js_str(&w, "Name"), &key, &format!("Wi-Fi ({})", util::js_str(&w, "Auth")), if key.is_empty() { "Open network, or run as administrator to read the key." } else { "" }, !key.is_empty()));
    }
    v
}

#[cfg(not(windows))]
pub fn list() -> Vec<KeyItem> {
    let mut v = Vec::new();
    // SSH host keys (fingerprints - what you compare on first connect).
    if let Ok(rd) = std::fs::read_dir("/etc/ssh") {
        let mut pubs: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.to_string_lossy().ends_with("_key.pub")).collect();
        pubs.sort();
        for p in pubs {
            if let Ok((0, out)) = util::run_capture("ssh-keygen", &["-lf", &p.to_string_lossy()]) {
                let f: Vec<&str> = out.split_whitespace().collect();
                let fp = f.get(1).copied().unwrap_or("");
                let alg = f.last().copied().unwrap_or("").trim_matches(['(', ')']);
                v.push(item("ssh", &format!("SSH host key ({alg})"), fp, &p.to_string_lossy(), &format!("{} bits", f.first().unwrap_or(&"")), false));
            }
        }
    }
    // Proxmox subscription.
    if std::path::Path::new("/usr/bin/pvesubscription").exists() {
        if let Ok((_, out)) = util::run_capture("pvesubscription", &["get"]) {
            let get = |k: &str| out.lines().find_map(|l| l.trim().strip_prefix(&format!("{k}:"))).unwrap_or("").trim().to_string();
            let key = get("key");
            v.push(item("proxmox", "Proxmox VE subscription", &key, "pvesubscription", &format!("status: {}{}", get("status"), if get("nextduedate").is_empty() { String::new() } else { format!(", next due {}", get("nextduedate")) }), !key.is_empty()));
        }
    }
    // Unraid license file.
    if let Ok(rd) = std::fs::read_dir("/boot/config") {
        for e in rd.flatten().filter(|e| e.path().extension().map(|x| x == "key").unwrap_or(false)) {
            v.push(item("unraid", "Unraid license key file", &e.path().to_string_lossy(), "/boot/config", "Back this file up - it is your Unraid license.", false));
        }
    }
    // WireGuard public keys.
    if let Ok((0, out)) = util::run_capture("wg", &["show", "all", "public-key"]) {
        for l in out.lines() {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() == 2 {
                v.push(item("wireguard", &format!("WireGuard {}", f[0]), f[1], "wg show", "Public key - safe to share with peers.", false));
            }
        }
    }
    for w in crate::toolkit::wifi().as_array().cloned().unwrap_or_default() {
        let key = util::js_str(&w, "Key");
        v.push(item("wifi", &util::js_str(&w, "Name"), &key, "NetworkManager", if key.is_empty() { "Run as root to read the key." } else { "" }, !key.is_empty()));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_known_digital_product_id() {
        // Encoded payload of the public generic Windows 10/11 Pro KMS client key
        // W269N-WFGWX-YVC9B-4J6C9-T83GX, produced by the inverse of the decoder.
        let key = "W269NWFGWXYVC9B4J6C9T83GX";
        let dpid = encode_for_test(key);
        assert_eq!(decode_product_key(&dpid).unwrap(), "W269N-WFGWX-YVC9B-4J6C9-T83GX");
    }

    /// Inverse of `decode_product_key` (Win8+ format) used only by the test.
    fn encode_for_test(key: &str) -> Vec<u8> {
        const CHARS: &[u8; 24] = b"BCDFGHJKMPQRTVWXY2346789";
        let pos = key.find('N').unwrap();
        let mut s: Vec<u8> = key.bytes().collect();
        s.remove(pos);
        // First char is a placeholder whose digit becomes `last` (= pos).
        let mut digits = vec![pos as u32];
        digits.extend(s.iter().map(|c| CHARS.iter().position(|x| x == c).unwrap() as u32));
        // digits[0] is the most significant base-24 digit.
        let mut k = [0u8; 15];
        for d in digits {
            let mut carry = d;
            for b in k.iter_mut() {
                let v = *b as u32 * 24 + carry;
                *b = (v & 0xFF) as u8;
                carry = v >> 8;
            }
        }
        k[14] = (k[14] & 0xF7) | 0x08; // win8 flag
        let mut dpid = vec![0u8; 52];
        dpid.extend_from_slice(&k);
        dpid.resize(164, 0);
        dpid
    }
}
