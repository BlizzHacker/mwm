//! Malware Lab - fast static triage of one file, native and offline:
//! hashes, PE/ELF/Mach-O structure, entropy, packer hints, mitigations,
//! risky imports -> capabilities -> MITRE ATT&CK, strings/IOCs, and matching
//! against the malware-family signatures from Arkana (MIT, (c) JameZUK /
//! BlizzHacker - see data/ARKANA-LICENSE.txt). Deep work (decompilation,
//! emulation, debugging) is handed to the user's own Arkana (`arkana.rs`).
//!
//! Also: quarantine / restore and an AV second opinion (Defender / ClamAV).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use aho_corasick::AhoCorasick;
use regex::bytes::Regex;
use serde::{Deserialize, Serialize};

use crate::util;

const MAX_READ: usize = 64 << 20;

// ------------------------------------------------------------ Arkana data ----

#[derive(Debug, Clone, Deserialize)]
struct Family {
    family: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    mitre: Vec<String>,
    #[serde(default)]
    strings: Vec<String>,
    #[serde(default)]
    hex: Vec<String>,
    #[serde(default)]
    hashes: Vec<u32>,
    #[serde(default)]
    commands: Vec<String>,
    #[serde(default)]
    source: String,
}

fn families() -> &'static Vec<Family> {
    static F: OnceLock<Vec<Family>> = OnceLock::new();
    F.get_or_init(|| {
        // The broad offline malware corpus contains known threat strings. On
        // Windows, leave family verdicts to Defender or the user's Arkana
        // instance; MWM still provides file structure, hashes, and IOCs.
        #[cfg(windows)]
        { Vec::new() }
        #[cfg(not(windows))]
        { serde_json::from_str(include_str!("../data/families.json")).unwrap_or_default() }
    })
}

/// Arkana's CATEGORIZED_IMPORTS_DB: api -> (risk, category).
fn risky_apis() -> &'static HashMap<String, (String, String)> {
    static M: OnceLock<HashMap<String, (String, String)>> = OnceLock::new();
    M.get_or_init(|| {
        let rows: Vec<(String, String, String)> = serde_json::from_str(include_str!("../data/imports.json")).unwrap_or_default();
        rows.into_iter().map(|(api, risk, cat)| (strip_aw(&api).to_string(), (risk, cat))).collect()
    })
}

fn strip_aw(api: &str) -> &str {
    let t = api.strip_suffix("ExA").or_else(|| api.strip_suffix("ExW")).map(|_| api).unwrap_or(api);
    if (t.ends_with('A') || t.ends_with('W')) && t.len() > 3 && t.as_bytes()[t.len() - 2].is_ascii_lowercase() {
        &t[..t.len() - 1]
    } else {
        t
    }
}

/// Category -> (ATT&CK id, name, plain-English meaning).
fn mitre_for(cat: &str) -> (&'static str, &'static str, &'static str) {
    match cat {
        "process_injection" => ("T1055", "Process Injection", "can inject code into other programs"),
        "credential_theft" => ("T1003", "OS Credential Dumping", "can dump passwords / credentials"),
        "privilege_escalation" => ("T1134", "Access Token Manipulation", "can raise its own privileges"),
        "anti_analysis" => ("T1622", "Debugger Evasion", "checks whether it is being analyzed"),
        "keylogging" => ("T1056.001", "Keylogging", "can record keystrokes"),
        "networking" => ("T1071", "Application Layer Protocol", "talks to the internet (possible command & control)"),
        "persistence" => ("T1543.003", "Create or Modify System Process", "can install itself to survive reboots"),
        "execution" => ("T1106", "Native API", "launches other programs / commands"),
        "registry" => ("T1112", "Modify Registry", "changes the registry"),
        "crypto" => ("T1027", "Obfuscated Files or Information", "uses encryption (data hiding or ransomware)"),
        "process_enumeration" => ("T1057", "Process Discovery", "lists running programs"),
        "process_manipulation" => ("T1489", "Service Stop", "can stop other programs / services"),
        "clipboard_access" => ("T1115", "Clipboard Data", "reads the clipboard"),
        "file_io" => ("T1083", "File and Directory Discovery", "reads and searches files"),
        "memory" => ("T1055", "Process Injection", "allocates executable memory"),
        _ => ("", "", ""),
    }
}

// ----------------------------------------------------------------- types ----

#[derive(Debug, Clone, Serialize, Default)]
pub struct Section {
    pub name: String,
    pub vsize: u64,
    pub rsize: u64,
    pub entropy: f64,
    pub perms: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Capability {
    pub category: String,
    pub risk: String,
    pub apis: Vec<String>,
    pub mitre: String,
    pub technique: String,
    pub meaning: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Ioc {
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FamilyHit {
    pub family: String,
    pub aliases: Vec<String>,
    pub confidence: u32,
    pub evidence: Vec<String>,
    pub mitre: Vec<String>,
    pub description: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Analysis {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub md5: String,
    pub sha1: String,
    pub sha256: String,
    pub format: String,
    pub arch: String,
    pub kind: String,
    pub entropy: f64,
    pub compiled: String,
    pub subsystem: String,
    pub dotnet: bool,
    pub signature: String,
    pub signer: String,
    pub pdb: String,
    pub mitigations: Vec<(String, bool)>,
    pub sections: Vec<Section>,
    pub imports: BTreeMap<String, Vec<String>>,
    pub import_count: usize,
    pub exports: Vec<String>,
    pub capabilities: Vec<Capability>,
    pub iocs: Vec<Ioc>,
    pub suspicious_strings: Vec<String>,
    pub string_count: usize,
    pub families: Vec<FamilyHit>,
    pub packer: Vec<String>,
    pub overlay: u64,
    pub score: u32,
    pub verdict: String,
    pub reasons: Vec<String>,
}

// ------------------------------------------------------------- analysis ----

pub fn entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut c = [0u64; 256];
    for b in data {
        c[*b as usize] += 1;
    }
    let n = data.len() as f64;
    c.iter().filter(|x| **x > 0).map(|x| {
        let p = *x as f64 / n;
        -p * p.log2()
    }).sum()
}

fn hashes(data: &[u8]) -> (String, String, String) {
    use md5::Digest as _;
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    (hex(&md5::Md5::digest(data)), hex(&sha1::Sha1::digest(data)), hex(&sha2::Sha256::digest(data)))
}

fn sniff(data: &[u8], name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if data.starts_with(b"MZ") {
        "PE"
    } else if data.starts_with(b"\x7fELF") {
        "ELF"
    } else if data.len() > 4 && matches!(&data[..4], [0xfe, 0xed, 0xfa, 0xce] | [0xce, 0xfa, 0xed, 0xfe] | [0xfe, 0xed, 0xfa, 0xcf] | [0xcf, 0xfa, 0xed, 0xfe] | [0xca, 0xfe, 0xba, 0xbe]) {
        "Mach-O"
    } else if data.starts_with(b"PK\x03\x04") {
        "ZIP archive (also .docx/.xlsx/.jar/.apk)"
    } else if data.starts_with(b"%PDF") {
        "PDF document"
    } else if data.starts_with(b"\xd0\xcf\x11\xe0") {
        "OLE / legacy Office document"
    } else if data.starts_with(b"Rar!") {
        "RAR archive"
    } else if data.starts_with(b"7z\xbc\xaf") {
        "7-Zip archive"
    } else if data.starts_with(b"#!") || [".ps1", ".bat", ".cmd", ".vbs", ".js", ".sh", ".py", ".hta", ".wsf"].iter().any(|e| lower.ends_with(e)) {
        "Script"
    } else if data.starts_with(b"L\0\0\0") {
        "Windows shortcut (.lnk)"
    } else {
        "Data"
    }
}

fn perms(r: bool, w: bool, x: bool) -> String {
    format!("{}{}{}", if r { "R" } else { "-" }, if w { "W" } else { "-" }, if x { "X" } else { "-" })
}

fn fmt_ts(secs: u64) -> String {
    // Days-from-civil without pulling in chrono.
    let days = (secs / 86400) as i64;
    let (z, rem) = (days + 719468, secs % 86400);
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", rem / 3600, rem % 3600 / 60)
}

fn parse_pe(a: &mut Analysis, data: &[u8], pe: &goblin::pe::PE) {
    let c = &pe.header.coff_header;
    a.format = if pe.is_64 { "PE32+ (64-bit)" } else { "PE32 (32-bit)" }.into();
    a.arch = match c.machine {
        0x14c => "x86",
        0x8664 => "x64",
        0xaa64 => "ARM64",
        0x1c4 => "ARM",
        _ => "other",
    }
    .into();
    a.kind = if pe.is_lib { "DLL" } else { "Executable" }.into();
    if c.time_date_stamp > 0 {
        a.compiled = fmt_ts(c.time_date_stamp as u64);
    }
    if let Some(oh) = pe.header.optional_header {
        let wf = &oh.windows_fields;
        a.subsystem = match wf.subsystem {
            2 => "Windows GUI",
            3 => "Console",
            1 => "Native / driver",
            _ => "other",
        }
        .into();
        let dc = wf.dll_characteristics;
        a.mitigations = vec![
            ("ASLR".into(), dc & 0x0040 != 0),
            ("High-entropy ASLR".into(), dc & 0x0020 != 0),
            ("DEP / NX".into(), dc & 0x0100 != 0),
            ("Control Flow Guard".into(), dc & 0x4000 != 0),
            ("No SEH".into(), dc & 0x0400 != 0),
        ];
        a.dotnet = oh.data_directories.get_clr_runtime_header().map(|d| d.size > 0).unwrap_or(false);
        let cert = oh.data_directories.get_certificate_table().map(|d| d.size > 0).unwrap_or(false);
        a.signature = if cert { "Signature present" } else { "Not signed" }.into();
    }
    if let Some(dbg) = &pe.debug_data {
        if let Some(cv) = &dbg.codeview_pdb70_debug_info {
            a.pdb = String::from_utf8_lossy(cv.filename).trim_end_matches('\0').to_string();
        }
    }
    let mut end_of_image = 0u64;
    for s in &pe.sections {
        let name = s.name().unwrap_or("?").to_string();
        let start = s.pointer_to_raw_data as usize;
        let len = s.size_of_raw_data as usize;
        end_of_image = end_of_image.max(start as u64 + len as u64);
        let bytes = data.get(start..start.saturating_add(len).min(data.len())).unwrap_or(&[]);
        let ch = s.characteristics;
        a.sections.push(Section {
            name,
            vsize: s.virtual_size as u64,
            rsize: s.size_of_raw_data as u64,
            entropy: (entropy(bytes) * 100.0).round() / 100.0,
            perms: perms(ch & 0x4000_0000 != 0, ch & 0x8000_0000 != 0, ch & 0x2000_0000 != 0),
        });
    }
    if (data.len() as u64) > end_of_image && end_of_image > 0 {
        a.overlay = data.len() as u64 - end_of_image;
    }
    for i in &pe.imports {
        a.imports.entry(i.dll.to_lowercase()).or_default().push(i.name.to_string());
    }
    a.exports = pe.exports.iter().filter_map(|e| e.name.map(String::from)).take(200).collect();
}

fn parse_elf(a: &mut Analysis, elf: &goblin::elf::Elf) {
    use goblin::elf::program_header::{PT_GNU_RELRO, PT_GNU_STACK, PT_INTERP};
    a.format = if elf.is_64 { "ELF64" } else { "ELF32" }.into();
    a.arch = match elf.header.e_machine {
        62 => "x86-64",
        3 => "x86",
        183 => "AArch64",
        40 => "ARM",
        8 => "MIPS",
        243 => "RISC-V",
        _ => "other",
    }
    .into();
    let has_interp = elf.program_headers.iter().any(|p| p.p_type == PT_INTERP);
    a.kind = match elf.header.e_type {
        2 => "Executable",
        3 if has_interp => "Executable (PIE)",
        3 => "Shared library",
        1 => "Object / kernel module",
        _ => "other",
    }
    .into();
    let nx = elf.program_headers.iter().find(|p| p.p_type == PT_GNU_STACK).map(|p| p.p_flags & 1 == 0).unwrap_or(false);
    let relro = elf.program_headers.iter().any(|p| p.p_type == PT_GNU_RELRO);
    let now = elf.dynamic.as_ref().map(|d| d.info.flags & 0x8 != 0 || d.info.flags_1 & 0x1 != 0).unwrap_or(false);
    let syms: Vec<String> = elf.dynsyms.iter().filter_map(|s| elf.dynstrtab.get_at(s.st_name).map(String::from)).collect();
    a.mitigations = vec![
        ("PIE".into(), elf.header.e_type == 3),
        ("NX stack".into(), nx),
        ("RELRO".into(), relro),
        ("Full RELRO".into(), relro && now),
        ("Stack canary".into(), syms.iter().any(|s| s == "__stack_chk_fail")),
        ("Stripped".into(), elf.syms.is_empty()),
    ];
    for s in &elf.section_headers {
        let name = elf.shdr_strtab.get_at(s.sh_name).unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        a.sections.push(Section {
            name,
            vsize: s.sh_size,
            rsize: s.sh_size,
            entropy: 0.0,
            perms: perms(true, s.sh_flags & 0x1 != 0, s.sh_flags & 0x4 != 0),
        });
    }
    let libs: Vec<String> = elf.libraries.iter().map(|s| s.to_string()).collect();
    let undefined: Vec<String> = elf.dynsyms.iter().filter(|s| s.st_shndx == 0).filter_map(|s| elf.dynstrtab.get_at(s.st_name).map(String::from)).filter(|s| !s.is_empty()).collect();
    a.imports.insert(if libs.is_empty() { "(static)".into() } else { libs.join(", ") }, undefined);
    a.exports = elf.dynsyms.iter().filter(|s| s.st_shndx != 0 && s.st_bind() == 1).filter_map(|s| elf.dynstrtab.get_at(s.st_name).map(String::from)).take(200).collect();
}

fn section_entropy_elf(a: &mut Analysis, data: &[u8], elf: &goblin::elf::Elf) {
    for (i, s) in elf.section_headers.iter().filter(|s| elf.shdr_strtab.get_at(s.sh_name).map(|n| !n.is_empty()).unwrap_or(false)).enumerate() {
        if s.sh_type == 8 {
            continue; // NOBITS (.bss)
        }
        let (o, l) = (s.sh_offset as usize, s.sh_size as usize);
        if let (Some(sec), Some(bytes)) = (a.sections.get_mut(i), data.get(o..o.saturating_add(l).min(data.len()))) {
            sec.entropy = (entropy(bytes) * 100.0).round() / 100.0;
        }
    }
}

// --------------------------------------------------------------- strings ----

fn ascii_and_utf16(data: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = Vec::new();
    for &b in data {
        if (0x20..0x7f).contains(&b) {
            cur.push(b);
        } else {
            if cur.len() >= 5 {
                out.push(String::from_utf8_lossy(&cur).into_owned());
            }
            cur.clear();
        }
    }
    if cur.len() >= 5 {
        out.push(String::from_utf8_lossy(&cur).into_owned());
    }
    // UTF-16LE: printable byte followed by 0.
    let mut i = 0;
    let mut w = String::new();
    while i + 1 < data.len() {
        let (lo, hi) = (data[i], data[i + 1]);
        if hi == 0 && (0x20..0x7f).contains(&lo) {
            w.push(lo as char);
            i += 2;
        } else {
            if w.len() >= 5 {
                out.push(std::mem::take(&mut w));
            }
            w.clear();
            i += 1;
        }
    }
    if w.len() >= 5 {
        out.push(w);
    }
    out
}

struct IocRes {
    url: Regex,
    ip: Regex,
    email: Regex,
    domain: Regex,
    reg: Regex,
    btc: Regex,
    eth: Regex,
    onion: Regex,
}

fn ioc_res() -> &'static IocRes {
    static R: OnceLock<IocRes> = OnceLock::new();
    R.get_or_init(|| IocRes {
        url: Regex::new(r#"(?i)\b(?:https?|ftp)://[a-z0-9\-._~%]+(?::\d+)?(?:/[^\s"'<>\x00]*)?"#).unwrap(),
        ip: Regex::new(r"\b(?:(?:25[0-5]|2[0-4]\d|1?\d?\d)\.){3}(?:25[0-5]|2[0-4]\d|1?\d?\d)\b").unwrap(),
        email: Regex::new(r"(?i)\b[a-z0-9._%+\-]{2,}@[a-z0-9.\-]+\.[a-z]{2,10}\b").unwrap(),
        domain: Regex::new(r"(?i)\b(?:[a-z0-9](?:[a-z0-9\-]{0,61}[a-z0-9])?\.)+(?:com|net|org|info|biz|ru|cn|top|xyz|io|cc|tk|pw|su|club|online|site|me|co|us|de|uk|in|br|onion|duckdns\.org|ngrok\.io)\b").unwrap(),
        reg: Regex::new(r"(?i)\b(?:HKEY_[A-Z_]+|HKLM|HKCU)\\[^\x00\r\n\t]{3,200}|\bSoftware\\Microsoft\\Windows\\CurrentVersion\\(?:Run|RunOnce|Policies)[^\x00\r\n]*").unwrap(),
        btc: Regex::new(r"\b(?:bc1[a-z0-9]{25,59}|[13][a-km-zA-HJ-NP-Z1-9]{25,34})\b").unwrap(),
        eth: Regex::new(r"\b0x[a-fA-F0-9]{40}\b").unwrap(),
        onion: Regex::new(r"(?i)\b[a-z2-7]{16,56}\.onion\b").unwrap(),
    })
}

const SUSPICIOUS: &[(&str, &str)] = &[
    ("powershell -enc", "encoded PowerShell"),
    ("powershell -e ", "encoded PowerShell"),
    ("-encodedcommand", "encoded PowerShell"),
    ("frombase64string", "decodes Base64 at runtime"),
    ("invoke-expression", "runs downloaded / generated code"),
    ("iex(", "runs downloaded / generated code"),
    ("downloadstring", "downloads and runs a script"),
    ("vssadmin delete shadows", "deletes backups (ransomware)"),
    ("wmic shadowcopy delete", "deletes backups (ransomware)"),
    ("bcdedit /set", "disables recovery (ransomware)"),
    ("wbadmin delete", "deletes backups (ransomware)"),
    ("schtasks /create", "creates a scheduled task (persistence)"),
    ("reg add", "writes the registry from a shell"),
    ("netsh advfirewall", "changes the firewall"),
    ("set-mppreference", "tampers with Defender"),
    ("disablerealtimemonitoring", "turns off Defender"),
    ("select * from antivirusproduct", "checks installed antivirus"),
    ("vboxservice", "VirtualBox / sandbox check"),
    ("vmtoolsd", "VMware / sandbox check"),
    ("sbiedll", "Sandboxie check"),
    ("wireshark", "looks for analysis tools"),
    ("procmon", "looks for analysis tools"),
    ("\\login data", "reads saved browser passwords"),
    ("\\cookies", "reads browser cookies"),
    ("wallet.dat", "targets crypto wallets"),
    ("\\exodus\\", "targets crypto wallets"),
    ("\\metamask", "targets crypto wallets"),
    ("\\telegram desktop\\tdata", "steals Telegram sessions"),
    ("discord\\local storage", "steals Discord tokens"),
    ("api.telegram.org/bot", "exfiltrates via Telegram bot"),
    ("discord.com/api/webhooks", "exfiltrates via Discord webhook"),
    ("pastebin.com/raw", "fetches payloads from Pastebin"),
    ("/dev/tcp/", "reverse shell"),
    ("nc -e", "reverse shell"),
    ("bash -i >&", "reverse shell"),
    ("chmod +x /tmp", "drops and runs a binary from /tmp"),
    ("xmrig", "cryptominer"),
    ("stratum+tcp://", "cryptominer pool"),
    ("your files have been encrypted", "ransom note"),
    ("readme_for_decrypt", "ransom note"),
    ("mimikatz", "credential theft tool"),
    ("sekurlsa", "credential theft tool"),
];

fn scan_strings(a: &mut Analysis, data: &[u8]) {
    let strings = ascii_and_utf16(data);
    a.string_count = strings.len();
    let r = ioc_res();
    let mut seen = BTreeSet::new();
    let mut push = |a: &mut Analysis, kind: &str, v: String| {
        if a.iocs.len() < 300 && seen.insert((kind.to_string(), v.clone())) {
            a.iocs.push(Ioc { kind: kind.into(), value: v });
        }
    };
    for s in &strings {
        let b = s.as_bytes();
        for m in r.url.find_iter(b) {
            let v = String::from_utf8_lossy(m.as_bytes()).trim_end_matches(['.', ',', ')', ';']).to_string();
            if !v.contains("schemas.microsoft.com") && !v.contains("www.w3.org") && !v.contains("digicert") && !v.contains("verisign") && !v.contains("symantec") && !v.contains("globalsign") && !v.contains("sectigo") {
                push(a, "url", v);
            }
        }
        for m in r.ip.find_iter(b) {
            let v = String::from_utf8_lossy(m.as_bytes()).to_string();
            let o: Vec<u8> = v.split('.').filter_map(|x| x.parse().ok()).collect();
            // Skip version-number lookalikes and non-routable noise.
            if o.len() == 4 && o[0] != 0 && o[0] != 255 && !(o[1] == 0 && o[2] == 0) && !s.to_lowercase().contains("version") {
                push(a, "ip", v);
            }
        }
        for m in r.email.find_iter(b) {
            push(a, "email", String::from_utf8_lossy(m.as_bytes()).to_string());
        }
        for m in r.onion.find_iter(b) {
            push(a, "onion", String::from_utf8_lossy(m.as_bytes()).to_string());
        }
        for m in r.reg.find_iter(b) {
            push(a, "registry", String::from_utf8_lossy(m.as_bytes()).to_string());
        }
        for m in r.eth.find_iter(b) {
            push(a, "wallet", String::from_utf8_lossy(m.as_bytes()).to_string());
        }
        if s.len() < 64 {
            for m in r.btc.find_iter(b) {
                let v = String::from_utf8_lossy(m.as_bytes()).to_string();
                if v.starts_with("bc1") || v.chars().any(|c| c.is_ascii_digit()) && v.chars().any(|c| c.is_ascii_uppercase()) && v.chars().any(|c| c.is_ascii_lowercase()) && v.len() >= 30 {
                    push(a, "wallet?", v);
                }
            }
        }
        for m in r.domain.find_iter(b) {
            let v = String::from_utf8_lossy(m.as_bytes()).to_lowercase();
            if !v.ends_with("microsoft.com") && !v.ends_with("windows.com") && !v.contains("w3.org") && !v.ends_with("digicert.com") && !v.ends_with("verisign.com") && !v.ends_with("globalsign.com") && !v.ends_with("sectigo.com") && !v.ends_with("symantec.com") && !v.ends_with("usertrust.com") && !v.ends_with("comodoca.com") && !v.contains("..") {
                push(a, "domain", v);
            }
        }
        let low = s.to_lowercase();
        for (needle, why) in SUSPICIOUS {
            if low.contains(needle) && a.suspicious_strings.len() < 60 {
                let t: String = s.chars().take(160).collect();
                let line = format!("{why}: {t}");
                if !a.suspicious_strings.contains(&line) {
                    a.suspicious_strings.push(line);
                }
            }
        }
    }
    // URLs make their domains redundant.
    let url_hosts: BTreeSet<String> = a.iocs.iter().filter(|i| i.kind == "url").filter_map(|i| i.value.split("://").nth(1).map(|h| h.split(['/', ':']).next().unwrap_or("").to_lowercase())).collect();
    a.iocs.retain(|i| !(i.kind == "domain" && url_hosts.contains(&i.value)));
}

// ------------------------------------------------------------- families ----

struct FamilyIndex {
    ac: AhoCorasick,
    /// pattern index -> (family index, label, weight)
    owners: Vec<(usize, String, u32)>,
}

fn family_index() -> &'static FamilyIndex {
    static I: OnceLock<FamilyIndex> = OnceLock::new();
    I.get_or_init(|| {
        let mut pats: Vec<Vec<u8>> = Vec::new();
        let mut owners = Vec::new();
        for (fi, f) in families().iter().enumerate() {
            let mut add = |bytes: Vec<u8>, label: String, w: u32| {
                if bytes.len() >= 4 {
                    pats.push(bytes);
                    owners.push((fi, label, w));
                }
            };
            for s in &f.strings {
                add(s.as_bytes().to_vec(), format!("string \"{s}\""), 3);
                add(s.encode_utf16().flat_map(|c| c.to_le_bytes()).collect(), format!("string \"{s}\" (UTF-16)"), 3);
            }
            for c in &f.commands {
                add(c.as_bytes().to_vec(), format!("command \"{c}\""), 1);
            }
            for h in &f.hashes {
                add(h.to_le_bytes().to_vec(), format!("API hash 0x{h:08x}"), 2);
            }
            for hx in &f.hex {
                if let Ok(bytes) = (0..hx.len()).step_by(2).map(|i| u8::from_str_radix(hx.get(i..i + 2).unwrap_or("zz"), 16)).collect::<Result<Vec<u8>, _>>() {
                    if bytes.len() >= 8 {
                        add(bytes, "byte pattern".into(), 4);
                    }
                }
            }
        }
        FamilyIndex { ac: AhoCorasick::new(&pats).expect("patterns"), owners }
    })
}

fn match_families(a: &mut Analysis, data: &[u8]) {
    let idx = family_index();
    let mut hits: HashMap<usize, BTreeMap<String, u32>> = HashMap::new();
    for m in idx.ac.find_overlapping_iter(data) {
        let (fi, label, w) = &idx.owners[m.pattern().as_usize()];
        hits.entry(*fi).or_default().insert(label.clone(), *w);
    }
    for (fi, ev) in hits {
        let f = &families()[fi];
        let strings = ev.keys().filter(|k| k.starts_with("string")).count();
        let hashes = ev.keys().filter(|k| k.starts_with("API hash")).count();
        let bytes = ev.keys().filter(|k| k.starts_with("byte")).count();
        let commands = ev.keys().filter(|k| k.starts_with("command")).count();
        // Demand corroboration - single generic words match everything.
        let strong = strings >= 2 || hashes >= 3 || bytes >= 1 || (strings >= 1 && (hashes >= 1 || commands >= 3));
        if !strong {
            continue;
        }
        let possible = (f.strings.len() * 3 + f.hashes.len() * 2 + f.commands.len()).max(1) as f64;
        let got: u32 = ev.values().sum();
        let confidence = ((got as f64 / possible) * 100.0).clamp(20.0, 95.0) as u32;
        a.families.push(FamilyHit {
            family: f.family.clone(),
            aliases: f.aliases.clone(),
            confidence,
            evidence: ev.keys().take(12).cloned().collect(),
            mitre: f.mitre.clone(),
            description: f.description.clone(),
            source: f.source.clone(),
        });
    }
    a.families.sort_by(|x, y| y.confidence.cmp(&x.confidence));
    a.families.truncate(5);
}

// ------------------------------------------------------------ signature ----

#[cfg(windows)]
fn authenticode(a: &mut Analysis) {
    let p = a.path.replace('\'', "''");
    let script = format!("Get-AuthenticodeSignature -LiteralPath '{p}' | Select-Object @{{n='Status';e={{\"$($_.Status)\"}}}},@{{n='Signer';e={{$_.SignerCertificate.Subject}}}} | ConvertTo-Json -Compress");
    if let Some(v) = util::powershell_json(&script).ok().and_then(|v| v.into_iter().next()) {
        let st = util::js_str(&v, "Status");
        let signer = util::js_str(&v, "Signer");
        a.signature = match st.as_str() {
            "Valid" => "Valid signature".into(),
            "NotSigned" => "Not signed".into(),
            "HashMismatch" => "BROKEN signature (file was modified)".into(),
            "NotTrusted" => "Signed, but not trusted".into(),
            "" => a.signature.clone(),
            other => format!("Signature: {other}"),
        };
        a.signer = signer.split(',').find_map(|p| p.trim().strip_prefix("CN=")).unwrap_or(&signer).trim_matches('"').to_string();
    }
}

#[cfg(not(windows))]
fn authenticode(a: &mut Analysis) {
    if a.format.starts_with("ELF") {
        a.signature = match util::run_capture("dpkg-query", &["-S", &a.path]) {
            Ok((0, out)) => format!("Owned by package {}", out.split(':').next().unwrap_or("").trim()),
            _ => "Not from any installed package".into(),
        };
    }
}

// ---------------------------------------------------------------- score ----

fn score(a: &mut Analysis) {
    let mut s = 0u32;
    let mut why = Vec::new();
    for f in &a.families {
        s += 45;
        why.push(format!("Matches {} indicators ({}% confidence)", f.family, f.confidence));
    }
    let crit: Vec<&Capability> = a.capabilities.iter().filter(|c| c.risk == "CRITICAL").collect();
    if !crit.is_empty() {
        s += 15 * crit.len().min(3) as u32;
        why.push(format!("High-risk abilities: {}", crit.iter().map(|c| c.meaning.as_str()).collect::<Vec<_>>().join("; ")));
    }
    if a.capabilities.iter().any(|c| c.category == "keylogging") {
        s += 10;
    }
    let wx: Vec<&Section> = a.sections.iter().filter(|x| x.perms.contains('W') && x.perms.contains('X')).collect();
    if !wx.is_empty() {
        s += 10;
        why.push(format!("Writable + executable section ({}) - typical of packers / self-modifying code", wx.iter().map(|x| x.name.as_str()).collect::<Vec<_>>().join(", ")));
    }
    if !a.packer.is_empty() {
        s += 15;
        why.push(format!("Packed / obfuscated: {}", a.packer.join("; ")));
    }
    if !a.suspicious_strings.is_empty() {
        s += (a.suspicious_strings.len() as u32 * 6).min(30);
        why.push(format!("{} suspicious string(s), e.g. {}", a.suspicious_strings.len(), a.suspicious_strings[0]));
    }
    let net_iocs = a.iocs.iter().filter(|i| matches!(i.kind.as_str(), "url" | "ip" | "onion")).count();
    if net_iocs > 0 {
        s += (net_iocs as u32 * 2).min(10);
    }
    if a.signature.starts_with("Valid") {
        s = s.saturating_sub(25);
        why.push(format!("Validly signed by {}", if a.signer.is_empty() { "a trusted publisher" } else { &a.signer }));
    } else if a.signature.starts_with("BROKEN") {
        s += 25;
        why.push("Signature is broken - the file was tampered with after signing".into());
    } else if a.signature.starts_with("Owned by package") {
        s = s.saturating_sub(25);
        why.push(a.signature.clone());
    }
    a.score = s.min(100);
    a.verdict = if a.score >= 60 {
        "Likely malicious"
    } else if a.score >= 30 {
        "Suspicious"
    } else {
        "No obvious threats"
    }
    .into();
    a.reasons = why;
}

pub fn analyze(path: &str) -> anyhow::Result<Analysis> {
    let p = Path::new(path);
    let meta = std::fs::metadata(p)?;
    if meta.is_dir() {
        anyhow::bail!("pick a file, not a folder");
    }
    let mut data = Vec::new();
    use std::io::Read;
    std::fs::File::open(p)?.take(MAX_READ as u64).read_to_end(&mut data)?;
    let mut a = Analysis {
        path: path.into(),
        name: p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        size: meta.len(),
        entropy: (entropy(&data) * 100.0).round() / 100.0,
        ..Default::default()
    };
    (a.md5, a.sha1, a.sha256) = hashes(&data);
    a.format = sniff(&data, &a.name).into();
    match goblin::Object::parse(&data) {
        Ok(goblin::Object::PE(pe)) => parse_pe(&mut a, &data, &pe),
        Ok(goblin::Object::Elf(elf)) => {
            parse_elf(&mut a, &elf);
            section_entropy_elf(&mut a, &data, &elf);
        }
        Ok(goblin::Object::Mach(goblin::mach::Mach::Binary(m))) => {
            a.format = "Mach-O".into();
            a.kind = if m.header.filetype == 6 { "Dylib" } else { "Executable" }.into();
            if let Ok(imps) = m.imports() {
                for i in imps {
                    a.imports.entry(i.dylib.to_string()).or_default().push(i.name.to_string());
                }
            }
        }
        Ok(goblin::Object::Mach(_)) => a.format = "Mach-O universal".into(),
        _ => {}
    }
    a.import_count = a.imports.values().map(|v| v.len()).sum();

    // Capabilities from risky imports (Arkana's table).
    let db = risky_apis();
    let mut by_cat: BTreeMap<String, (String, BTreeSet<String>)> = BTreeMap::new();
    let pe_like = a.format.starts_with("PE");
    for f in a.imports.values().flatten() {
        let key = strip_aw(f);
        if let Some((risk, cat)) = db.get(key) {
            // connect/send/... are normal in ELF - only count them for PE.
            if !pe_like && cat == "networking" && ["connect", "send", "recv", "socket", "bind", "listen", "accept"].contains(&key) {
                continue;
            }
            let e = by_cat.entry(cat.clone()).or_insert_with(|| (risk.clone(), BTreeSet::new()));
            if risk == "CRITICAL" || (risk == "HIGH" && e.0 != "CRITICAL") {
                e.0 = risk.clone();
            }
            e.1.insert(f.clone());
        }
    }
    for (cat, (risk, apis)) in by_cat {
        let (id, name, meaning) = mitre_for(&cat);
        a.capabilities.push(Capability { category: cat, risk, apis: apis.into_iter().collect(), mitre: id.into(), technique: name.into(), meaning: meaning.into() });
    }
    a.capabilities.sort_by_key(|c| match c.risk.as_str() {
        "CRITICAL" => 0,
        "HIGH" => 1,
        _ => 2,
    });

    // Packer hints.
    let names: Vec<String> = a.sections.iter().map(|s| s.name.to_lowercase()).collect();
    if names.iter().any(|n| n.starts_with("upx")) {
        a.packer.push("UPX".into());
    }
    for (sig, label) in [(".aspack", "ASPack"), (".themida", "Themida"), (".vmp", "VMProtect"), (".enigma", "Enigma"), ("mpress", "MPRESS"), (".petite", "Petite"), (".nsp", "NsPack")] {
        if names.iter().any(|n| n.starts_with(sig)) {
            a.packer.push(label.into());
        }
    }
    if let Some(s) = a.sections.iter().find(|s| s.entropy > 7.2 && s.rsize > 4096) {
        a.packer.push(format!("very high entropy in {} ({:.2})", s.name, s.entropy));
    }
    if pe_like && a.import_count > 0 && a.import_count < 8 && !a.dotnet {
        a.packer.push(format!("only {} imports", a.import_count));
    }

    scan_strings(&mut a, &data);
    match_families(&mut a, &data);
    authenticode(&mut a);
    score(&mut a);
    Ok(a)
}

// ------------------------------------------------- quarantine / AV scan ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QItem {
    pub id: String,
    pub original: String,
    pub sha256: String,
    pub size: u64,
    pub when: u64,
    pub reason: String,
}

fn qdir() -> PathBuf {
    let d = util::data_dir().join("quarantine");
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn quarantine(path: &str, reason: &str) -> anyhow::Result<QItem> {
    if std::fs::metadata(path)?.len() > MAX_READ as u64 {
        anyhow::bail!("file is larger than the 64 MiB quarantine limit");
    }
    let data = std::fs::read(path)?;
    let (_, _, sha) = hashes(&data);
    let id = format!("{}-{}", util::now_secs(), &sha[..12]);
    // Stored XOR-ed so the payload can't run or trip AV from the quarantine folder.
    let neutered: Vec<u8> = data.iter().map(|b| b ^ 0xA5).collect();
    std::fs::write(qdir().join(format!("{id}.bin")), neutered)?;
    let item = QItem { id: id.clone(), original: path.into(), sha256: sha, size: data.len() as u64, when: util::now_secs(), reason: reason.into() };
    std::fs::write(qdir().join(format!("{id}.json")), serde_json::to_string_pretty(&item)?)?;
    std::fs::remove_file(path)?;
    Ok(item)
}

pub fn quarantine_list() -> Vec<QItem> {
    let mut v: Vec<QItem> = std::fs::read_dir(qdir())
        .map(|rd| rd.flatten().filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false)).filter_map(|e| std::fs::read_to_string(e.path()).ok()).filter_map(|s| serde_json::from_str(&s).ok()).collect())
        .unwrap_or_default();
    v.sort_by(|a, b| b.when.cmp(&a.when));
    v
}

pub fn quarantine_restore(id: &str) -> anyhow::Result<String> {
    validate_qid(id)?;
    let meta: QItem = serde_json::from_str(&std::fs::read_to_string(qdir().join(format!("{id}.json")))?)?;
    let data: Vec<u8> = std::fs::read(qdir().join(format!("{id}.bin")))?.iter().map(|b| b ^ 0xA5).collect();
    let target = if Path::new(&meta.original).exists() { format!("{}.restored", meta.original) } else { meta.original.clone() };
    std::fs::write(&target, data)?;
    quarantine_delete(id)?;
    Ok(target)
}

pub fn quarantine_delete(id: &str) -> anyhow::Result<()> {
    validate_qid(id)?;
    for ext in ["bin", "json"] {
        let _ = std::fs::remove_file(qdir().join(format!("{id}.{ext}")));
    }
    Ok(())
}

fn validate_qid(id: &str) -> anyhow::Result<()> {
    let (when, hash) = id.split_once('-').ok_or_else(|| anyhow::anyhow!("invalid quarantine ID"))?;
    if when.is_empty() || !when.bytes().all(|b| b.is_ascii_digit()) || hash.len() != 12 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        anyhow::bail!("invalid quarantine ID");
    }
    Ok(())
}

/// Second opinion from the platform antivirus (Defender / ClamAV).
pub fn av_scan(path: &str) -> anyhow::Result<String> {
    #[cfg(windows)]
    {
        let exe = util::env_path("ProgramFiles").unwrap_or_default().join(r"Windows Defender\MpCmdRun.exe");
        let (code, out) = util::run_capture(&exe.to_string_lossy(), &["-Scan", "-ScanType", "3", "-File", path, "-DisableRemediation"])?;
        let lines: Vec<&str> = out.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        return Ok(match code {
            0 => "Microsoft Defender: no threats found.".into(),
            2 => format!("Microsoft Defender: THREAT FOUND - {}", lines.iter().filter(|l| l.contains("Threat") || l.contains("threat")).cloned().collect::<Vec<_>>().join(" / ")),
            c => format!("Microsoft Defender exited with {c}: {}", lines.last().unwrap_or(&"")),
        });
    }
    #[cfg(not(windows))]
    {
        match util::run_capture("clamscan", &["--no-summary", path]) {
            Ok((0, _)) => Ok("ClamAV: clean.".into()),
            Ok((1, out)) => Ok(format!("ClamAV: THREAT FOUND - {}", out.trim())),
            Ok((_, out)) => Ok(format!("ClamAV error: {}", out.trim())),
            Err(_) => Ok("ClamAV is not installed (apt install clamav) - static triage only.".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_loads() {
        #[cfg(not(windows))]
        assert!(families().len() > 100, "families: {}", families().len());
        assert!(risky_apis().contains_key("CreateRemoteThread"));
        assert_eq!(strip_aw("RegSetValueExW"), "RegSetValueEx");
        assert_eq!(strip_aw("VirtualAlloc"), "VirtualAlloc");
    }

    #[test]
    fn analyzes_own_binary_and_flags_fake_sample() {
        let me = std::env::current_exe().unwrap();
        let a = analyze(&me.to_string_lossy()).unwrap();
        assert!(a.format.starts_with("ELF") || a.format.starts_with("PE"), "{}", a.format);
        assert_eq!(a.sha256.len(), 64);
        assert!(!a.sections.is_empty());

        let p = std::env::temp_dir().join(format!("mwm-lab-{}.txt", std::process::id()));
        std::fs::write(&p, b"powershell -enc AAAA\nhttp://evil.example.top/payload.bin\nvssadmin delete shadows /all\nAsyncClient ServerSignature Pastebin\0").unwrap();
        let b = analyze(&p.to_string_lossy()).unwrap();
        assert!(b.iocs.iter().any(|i| i.kind == "url"), "{:?}", b.iocs);
        assert!(b.suspicious_strings.len() >= 2);
        #[cfg(not(windows))]
        assert!(b.families.iter().any(|f| f.family == "AsyncRAT"), "{:?}", b.families);
        #[cfg(not(windows))]
        assert!(b.score >= 45, "score {}", b.score);
        #[cfg(windows)]
        assert!(b.score > 0, "score {}", b.score);
        let q = quarantine(&p.to_string_lossy(), "test").unwrap();
        assert!(!p.exists());
        assert_eq!(quarantine_restore(&q.id).unwrap(), p.to_string_lossy());
        assert!(p.exists());
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn timestamps() {
        assert_eq!(fmt_ts(0), "1970-01-01 00:00 UTC");
        assert_eq!(fmt_ts(1_790_000_000), "2026-09-21 14:13 UTC");
    }
}
