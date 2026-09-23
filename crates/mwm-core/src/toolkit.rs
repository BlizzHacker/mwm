//! Tech Toolkit - the Geek Squad MRI / Hiren's replacement: system report,
//! disk health, security status, crash/event log, network diagnostics, Wi-Fi
//! keys, and one-click repair tasks that run as background jobs.
//!
//! Everything uses tools the OS already ships (PowerShell/CIM, sfc, DISM,
//! netsh, MpCmdRun...). No third-party binaries, nothing downloaded.

use serde::Serialize;
use serde_json::{json, Value};

use crate::jobs;
#[allow(unused_imports)]
use crate::util;

#[derive(Debug, Clone, Serialize)]
pub struct Task {
    pub id: &'static str,
    pub group: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub command: &'static str,
    pub admin: bool,
    pub minutes: &'static str,
    pub warning: &'static str,
}

const fn t(
    id: &'static str,
    group: &'static str,
    name: &'static str,
    description: &'static str,
    command: &'static str,
    admin: bool,
    minutes: &'static str,
    warning: &'static str,
) -> Task {
    Task { id, group, name, description, command, admin, minutes, warning }
}

#[cfg(windows)]
pub const TASKS: &[Task] = &[
    t("sfc", "Windows repair", "System File Checker", "Finds and repairs corrupted Windows system files (sfc /scannow).", "sfc /scannow", true, "10-30", ""),
    t("dism", "Windows repair", "Repair Windows image", "DISM RestoreHealth - repairs the component store that SFC repairs from. Needs internet.", "DISM /Online /Cleanup-Image /RestoreHealth", true, "15-40", ""),
    t("dismclean", "Windows repair", "Clean up component store", "Removes superseded update files from WinSxS. Often frees several GB.", "DISM /Online /Cleanup-Image /StartComponentCleanup", true, "5-20", ""),
    t("chkdsk", "Windows repair", "Check disk (online)", "Scans C: for file-system errors without rebooting.", "chkdsk C: /scan", true, "2-15", ""),
    t("wureset", "Windows repair", "Reset Windows Update", "Stops update services, sets the download cache aside, restarts them. Fixes stuck updates.", "net stop wuauserv /y & net stop bits /y & ren \"%windir%\\SoftwareDistribution\" SoftwareDistribution.mwm-%RANDOM% & net start bits & net start wuauserv", true, "1", ""),
    t("icons", "Windows repair", "Rebuild icon cache", "Fixes blank or wrong icons in Explorer and the Start menu.", "ie4uinit.exe -show", false, "<1", ""),
    t("restore", "Windows repair", "Create restore point", "Snapshot of system settings and drivers - make one before big changes.", "powershell -NoProfile -Command \"Checkpoint-Computer -Description 'MWM restore point' -RestorePointType MODIFY_SETTINGS; 'Restore point created.'\"", true, "1-3", ""),
    t("time", "Windows repair", "Resync clock", "Fixes certificate and sign-in errors caused by a wrong clock.", "net start w32time & w32tm /resync /force", true, "<1", ""),
    t("dns", "Network", "Flush DNS cache", "Fixes sites that won't load after DNS changes.", "ipconfig /flushdns", false, "<1", ""),
    t("renew", "Network", "Renew IP address", "Releases and renews DHCP leases. Connection drops for a few seconds.", "ipconfig /release & ipconfig /renew", false, "<1", "Your connection drops for a few seconds."),
    t("netreset", "Network", "Reset network stack", "Resets Winsock and TCP/IP to defaults - the classic 'internet is broken' fix.", "netsh winsock reset & netsh int ip reset", true, "<1", "Restart the PC afterwards."),
    t("spooler", "Printing", "Fix stuck printing", "Clears the print queue and restarts the Print Spooler.", "net stop spooler /y & del /q /f /s \"%windir%\\System32\\spool\\PRINTERS\\*\" & net start spooler", true, "<1", ""),
    t("defupdate", "Security", "Update Defender", "Downloads the latest Microsoft Defender definitions.", "\"%ProgramFiles%\\Windows Defender\\MpCmdRun.exe\" -SignatureUpdate", false, "1-3", ""),
    t("defquick", "Security", "Defender quick scan", "Scans the places malware usually hides.", "\"%ProgramFiles%\\Windows Defender\\MpCmdRun.exe\" -Scan -ScanType 1", false, "2-10", ""),
    t("deffull", "Security", "Defender full scan", "Scans every file on every drive.", "\"%ProgramFiles%\\Windows Defender\\MpCmdRun.exe\" -Scan -ScanType 2", false, "30-180", ""),
    t("battery", "Hardware", "Battery health report", "Design vs. full-charge capacity and usage history (opens in your browser).", "powercfg /batteryreport /output \"%LOCALAPPDATA%\\MoveWeight\\MWM\\battery-report.html\" && start \"\" \"%LOCALAPPDATA%\\MoveWeight\\MWM\\battery-report.html\"", false, "<1", ""),
    t("energy", "Hardware", "Power efficiency report", "60-second trace of what drains power / blocks sleep.", "powercfg /energy /output \"%LOCALAPPDATA%\\MoveWeight\\MWM\\energy-report.html\" /duration 60 & start \"\" \"%LOCALAPPDATA%\\MoveWeight\\MWM\\energy-report.html\"", true, "1", ""),
    t("memtest", "Hardware", "Memory test (on reboot)", "Schedules Windows Memory Diagnostic at next restart.", "mdsched.exe", true, "10-20", "Opens a dialog - choose when to restart."),
];

#[cfg(not(windows))]
pub const TASKS: &[Task] = &[
    t("aptfix", "Packages", "Fix broken packages", "Finishes interrupted installs and repairs dependencies.", "dpkg --configure -a && apt-get -f install -y", true, "1-5", ""),
    t("aptupdate", "Packages", "Refresh package lists", "apt-get update.", "apt-get update", true, "<1", ""),
    t("dns", "Network", "Flush DNS cache", "systemd-resolved cache flush.", "resolvectl flush-caches && echo flushed", true, "<1", ""),
    t("journal", "Logs", "Shrink journal to 200 MB", "journalctl --vacuum-size=200M.", "journalctl --vacuum-size=200M", true, "<1", ""),
    t("fstrim", "Disks", "TRIM SSDs", "Tells SSDs which blocks are free (fstrim -av).", "fstrim -av", true, "<1", ""),
    t("failed", "Services", "List failed services", "systemctl --failed.", "systemctl --failed --no-pager", false, "<1", ""),
    t("kernels", "Packages", "Remove old kernels", "apt autoremove --purge: drops kernels and packages nothing needs any more (the running kernel is kept).", "apt-get autoremove --purge -y", true, "1-5", "Reboot into the newest kernel first if you just upgraded."),
    t("upgradable", "Packages", "List available upgrades", "Shows what apt would upgrade (does not install).", "apt-get update -qq && apt list --upgradable 2>/dev/null", true, "<1", ""),
    t("dockerprune", "Docker", "Docker cleanup", "Removes stopped containers, unused networks, dangling images and build cache.", "docker system prune -f && docker system df", true, "1-5", "Stopped containers are deleted - start anything you want to keep first."),
    t("zscrub", "Storage", "Scrub all ZFS pools", "Reads every block and repairs silent corruption from redundancy.", "for p in $(zpool list -H -o name); do zpool scrub $p && echo scrub started on $p; done", true, "background", "Heavy disk IO until it finishes - best overnight."),
    t("smartall", "Storage", "SMART short test (all disks)", "Starts a ~2 minute self-test on every disk; results show on the Server page.", "for d in $(smartctl --scan | cut -d' ' -f1); do smartctl -t short $d | grep -i -E 'test|error' | head -2; done", true, "2", ""),
    t("logs", "Logs", "Biggest log files", "Top 20 largest files under /var/log.", "du -ah /var/log 2>/dev/null | sort -rh | head -20", true, "<1", ""),
];

#[cfg_attr(not(windows), allow(dead_code))]
/// Decode tool output that may be UTF-16 (sfc, DISM when redirected) or OEM.
fn decode(bytes: &[u8]) -> String {
    let zeros = bytes.iter().filter(|b| **b == 0).count();
    if zeros > bytes.len() / 4 {
        let start = if bytes.starts_with(&[0xFF, 0xFE]) { 2 } else { 0 };
        let u: Vec<u16> = bytes[start..].chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        String::from_utf8_lossy(bytes).replace('\0', "")
    }
}

/// Run a repair task as a background job, streaming its output.
pub fn run_task(id: &str) -> anyhow::Result<u64> {
    let task = TASKS.iter().find(|t| t.id == id).ok_or_else(|| anyhow::anyhow!("unknown task {id}"))?.clone();
    Ok(jobs::start("repair", task.name, "repair", move |job| {
        job.line(format!("> {}", task.command));
        let log = util::data_dir().join(format!("task-{}-{}.log", task.id, job.id));
        let _ = std::fs::remove_file(&log);
        let code = run_logged(job, task.command, task.admin, &log)?;
        let _ = std::fs::remove_file(&log);
        if code == 0 {
            Ok(format!("{} finished", task.name))
        } else {
            anyhow::bail!("{} exited with code {code}", task.name)
        }
    }))
}

/// Start `command` (elevated if needed), tee its output into `log`, and
/// stream new lines into the job while it runs.
#[cfg(windows)]
fn run_logged(job: &jobs::Job, command: &str, admin: bool, log: &std::path::Path) -> anyhow::Result<i32> {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, TerminateProcess, WaitForSingleObject};
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};

    // Parenthesised so the redirect captures every command in an `a & b` chain.
    let params = format!("/d /s /c \"({command}) > \"{}\" 2>&1\"", log.display());
    let w = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (verb, file, par) = (w("runas"), w("cmd.exe"), w(&params));
    let elevate = admin && !crate::sys::is_elevated();
    let h = unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
        info.lpVerb = if elevate { verb.as_ptr() } else { std::ptr::null() };
        info.lpFile = file.as_ptr();
        info.lpParameters = par.as_ptr();
        info.nShow = 0; // SW_HIDE
        if ShellExecuteExW(&mut info) == 0 {
            anyhow::bail!("could not start (administrator permission was declined?)");
        }
        info.hProcess
    };
    let mut seen = 0usize;
    let mut pump = |job: &jobs::Job, final_: bool| {
        let Ok(bytes) = std::fs::read(log) else { return };
        let text = decode(&bytes);
        let lines: Vec<&str> = text.split(['\n']).collect();
        let complete = if final_ { lines.len() } else { lines.len().saturating_sub(1) };
        for l in lines.iter().take(complete).skip(seen) {
            // sfc/DISM redraw progress with \r - keep the last frame only.
            let l = l.rsplit('\r').find(|s| !s.trim().is_empty()).unwrap_or("").trim();
            if !l.is_empty() {
                job.line(l.to_string());
            }
        }
        seen = seen.max(complete);
        if let Some(last) = lines.last() {
            let cur = last.rsplit('\r').find(|s| !s.trim().is_empty()).unwrap_or("").trim();
            if !cur.is_empty() {
                job.current(cur);
            }
        }
    };
    let code = loop {
        if unsafe { WaitForSingleObject(h, 500) } != WAIT_TIMEOUT {
            pump(job, true);
            let mut c = 0u32;
            unsafe {
                GetExitCodeProcess(h, &mut c);
                CloseHandle(h);
            }
            break c as i32;
        }
        pump(job, false);
        if job.cancelled() {
            unsafe {
                TerminateProcess(h, 1);
                CloseHandle(h);
            }
            anyhow::bail!("cancelled");
        }
    };
    Ok(code)
}

#[cfg(not(windows))]
fn run_logged(job: &jobs::Job, command: &str, _admin: bool, _log: &std::path::Path) -> anyhow::Result<i32> {
    use std::io::{BufRead, BufReader};
    let mut child = util::cmd("sh")
        .args(["-c", &format!("{command} 2>&1")])
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    if let Some(out) = child.stdout.take() {
        for l in BufReader::new(out).lines().map_while(Result::ok) {
            job.line(l);
            if job.cancelled() {
                let _ = child.kill();
                anyhow::bail!("cancelled");
            }
        }
    }
    Ok(child.wait()?.code().unwrap_or(-1))
}

// ------------------------------------------------------------- reports ----

#[cfg(windows)]
pub fn report() -> Value {
    let script = r#"
$cs=Get-CimInstance Win32_ComputerSystem; $bios=Get-CimInstance Win32_BIOS; $bb=Get-CimInstance Win32_BaseBoard; $os=Get-CimInstance Win32_OperatingSystem
$cpu=Get-CimInstance Win32_Processor | Select-Object -First 1
$lic=Get-CimInstance SoftwareLicensingProduct -Filter "PartialProductKey IS NOT NULL" | Where-Object { $_.Name -like 'Windows*' } | Select-Object -First 1
$oa3=(Get-CimInstance SoftwareLicensingService).OA3xOriginalProductKey
[pscustomobject]@{
 Manufacturer=$cs.Manufacturer; Model=$cs.Model; SystemType=$cs.SystemType; RamBytes=$cs.TotalPhysicalMemory
 BiosVendor=$bios.Manufacturer; BiosVersion=$bios.SMBIOSBIOSVersion; BiosDate=$(if($bios.ReleaseDate){$bios.ReleaseDate.ToString('yyyy-MM-dd')}); Serial=$bios.SerialNumber
 Board="$($bb.Manufacturer) $($bb.Product)"
 Cpu="$($cpu.Name)".Trim(); Cores=$cpu.NumberOfCores; Threads=$cpu.NumberOfLogicalProcessors; MaxMHz=$cpu.MaxClockSpeed
 Os=$os.Caption; OsVersion=$os.Version; Build=$os.BuildNumber; InstallDate=$os.InstallDate.ToString('yyyy-MM-dd'); LastBoot=$os.LastBootUpTime.ToString('yyyy-MM-dd HH:mm')
 Activation=$(switch($lic.LicenseStatus){1{'Activated'}0{'Unlicensed'}5{'Notification mode'}default{'Grace period / unknown'}})
 LicenseName=$lic.Description; PartialKey=$lic.PartialProductKey; OemKey=$oa3
 Gpus=@(Get-CimInstance Win32_VideoController | ForEach-Object { [pscustomobject]@{Name=$_.Name; Driver=$_.DriverVersion; Ram=$_.AdapterRAM} })
 Memory=@(Get-CimInstance Win32_PhysicalMemory | ForEach-Object { [pscustomobject]@{Size=$_.Capacity; Speed=$_.ConfiguredClockSpeed; Maker=$_.Manufacturer; Part="$($_.PartNumber)".Trim(); Slot=$_.DeviceLocator} })
 Disks=@(Get-PhysicalDisk | ForEach-Object { $r = $_ | Get-StorageReliabilityCounter -ErrorAction SilentlyContinue; [pscustomobject]@{Name=$_.FriendlyName; Media="$($_.MediaType)"; Bus="$($_.BusType)"; Size=$_.Size; Health="$($_.HealthStatus)"; Status="$($_.OperationalStatus)"; Temp=$r.Temperature; Wear=$r.Wear; PowerOnHours=$r.PowerOnHours; ReadErrors=$r.ReadErrorsTotal; WriteErrors=$r.WriteErrorsTotal} })
 Battery=@(Get-CimInstance Win32_Battery | ForEach-Object { [pscustomobject]@{Name=$_.Name; Charge=$_.EstimatedChargeRemaining; Status=$_.BatteryStatus} })
} | ConvertTo-Json -Depth 4 -Compress"#;
    util::powershell_json(script).ok().and_then(|v| v.into_iter().next()).unwrap_or_else(|| json!({}))
}

#[cfg(not(windows))]
pub fn report() -> Value {
    let r = |p: &str| std::fs::read_to_string(p).map(|s| s.trim().to_string()).unwrap_or_default();
    let cpu = r("/proc/cpuinfo").lines().find(|l| l.starts_with("model name")).and_then(|l| l.split(':').nth(1)).unwrap_or("").trim().to_string();
    let os = r("/etc/os-release").lines().find_map(|l| l.strip_prefix("PRETTY_NAME=")).unwrap_or("").trim_matches('"').to_string();
    let disks: Value = util::run_capture("lsblk", &["-J", "-b", "-d", "-o", "NAME,MODEL,SIZE,ROTA,TRAN"])
        .ok()
        .and_then(|(_, o)| serde_json::from_str::<Value>(&o).ok())
        .and_then(|v| v.get("blockdevices").cloned())
        .map(|arr| {
            Value::Array(
                arr.as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|d| json!({"Name": format!("{} {}", d["name"].as_str().unwrap_or(""), d["model"].as_str().unwrap_or("")).trim(), "Media": if d["rota"].as_bool().unwrap_or(false) { "HDD" } else { "SSD" }, "Bus": d["tran"], "Size": d["size"], "Health": ""}))
                    .collect(),
            )
        })
        .unwrap_or(json!([]));
    let s = sysinfo::System::new_all();
    json!({
        "Manufacturer": r("/sys/class/dmi/id/sys_vendor"), "Model": r("/sys/class/dmi/id/product_name"),
        "BiosVendor": r("/sys/class/dmi/id/bios_vendor"), "BiosVersion": r("/sys/class/dmi/id/bios_version"), "BiosDate": r("/sys/class/dmi/id/bios_date"),
        "Board": format!("{} {}", r("/sys/class/dmi/id/board_vendor"), r("/sys/class/dmi/id/board_name")),
        "Cpu": cpu, "Threads": s.cpus().len(), "RamBytes": s.total_memory(),
        "Os": os, "OsVersion": r("/proc/sys/kernel/osrelease"), "Disks": disks, "Gpus": [], "Memory": [], "Battery": [],
    })
}

#[cfg(windows)]
pub fn security() -> Value {
    let script = r#"
$s = Get-MpComputerStatus -ErrorAction SilentlyContinue
[pscustomobject]@{
 Available=[bool]$s; RealTime=$s.RealTimeProtectionEnabled; Antivirus=$s.AntivirusEnabled; Tamper=$s.IsTamperProtected
 SigVersion=$s.AntivirusSignatureVersion; SigUpdated=$(if($s.AntivirusSignatureLastUpdated){$s.AntivirusSignatureLastUpdated.ToString('yyyy-MM-dd HH:mm')})
 QuickScanAge=$s.QuickScanAge; FullScanAge=$s.FullScanAge
 Firewall=@(Get-NetFirewallProfile -ErrorAction SilentlyContinue | ForEach-Object { [pscustomobject]@{Name=$_.Name; Enabled=[bool]$_.Enabled} })
 Threats=@(Get-MpThreatDetection -ErrorAction SilentlyContinue | Sort-Object InitialDetectionTime -Descending | Select-Object -First 25 | ForEach-Object { [pscustomobject]@{When=$_.InitialDetectionTime.ToString('yyyy-MM-dd HH:mm'); ThreatID=$_.ThreatID; Resources=($_.Resources -join '; '); Cleaned=$_.ActionSuccess; Name=(Get-MpThreat -ThreatID $_.ThreatID -ErrorAction SilentlyContinue).ThreatName} })
 UAC=(Get-ItemProperty HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System -ErrorAction SilentlyContinue).EnableLUA
 BitLocker=@(Get-CimInstance -Namespace root\cimv2\Security\MicrosoftVolumeEncryption -ClassName Win32_EncryptableVolume -ErrorAction SilentlyContinue | ForEach-Object { [pscustomobject]@{Drive=$_.DriveLetter; Protection=$_.ProtectionStatus} })
} | ConvertTo-Json -Depth 4 -Compress"#;
    util::powershell_json(script).ok().and_then(|v| v.into_iter().next()).unwrap_or_else(|| json!({"Available": false}))
}

#[cfg(not(windows))]
pub fn security() -> Value {
    let ufw = util::run_capture("ufw", &["status"]).map(|(_, o)| o.lines().next().unwrap_or("").to_string()).unwrap_or_default();
    json!({"Available": false, "Firewall": [{"Name": "ufw", "Enabled": ufw.contains("active") && !ufw.contains("inactive")}], "Threats": []})
}

#[cfg(windows)]
pub fn events() -> Value {
    let script = r#"
$since=(Get-Date).AddDays(-7)
$errs=@(Get-WinEvent -FilterHashtable @{LogName='System','Application'; Level=1,2; StartTime=$since} -MaxEvents 200 -ErrorAction SilentlyContinue | ForEach-Object { [pscustomobject]@{Time=$_.TimeCreated.ToString('yyyy-MM-dd HH:mm'); Id=$_.Id; Source=$_.ProviderName; Level=$_.LevelDisplayName; Log=$_.LogName; Message=("$($_.Message)" -split "`n")[0].Trim()} })
$crash=@(Get-WinEvent -FilterHashtable @{LogName='System'; Id=41,1001,6008; StartTime=(Get-Date).AddDays(-60)} -MaxEvents 50 -ErrorAction SilentlyContinue | ForEach-Object { [pscustomobject]@{Time=$_.TimeCreated.ToString('yyyy-MM-dd HH:mm'); Id=$_.Id; Source=$_.ProviderName; Message=("$($_.Message)" -split "`n")[0].Trim()} })
$dumps=@(Get-ChildItem "$env:windir\Minidump" -Filter *.dmp -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | ForEach-Object { [pscustomobject]@{Name=$_.Name; Time=$_.LastWriteTime.ToString('yyyy-MM-dd HH:mm'); Size=$_.Length} })
[pscustomobject]@{Errors=$errs; Crashes=$crash; Dumps=$dumps} | ConvertTo-Json -Depth 4 -Compress"#;
    util::powershell_json(script).ok().and_then(|v| v.into_iter().next()).unwrap_or_else(|| json!({"Errors": [], "Crashes": [], "Dumps": []}))
}

#[cfg(not(windows))]
pub fn events() -> Value {
    let errs: Vec<Value> = util::run_capture("journalctl", &["-p", "3", "-S", "-7d", "-o", "short-iso", "--no-pager", "-n", "200"])
        .map(|(_, o)| o.lines().filter(|l| !l.starts_with("--")).map(|l| {
            let mut it = l.splitn(4, ' ');
            let time = it.next().unwrap_or("");
            let _host = it.next();
            let src = it.next().unwrap_or("").trim_end_matches(':');
            json!({"Time": time, "Id": "", "Source": src, "Level": "Error", "Message": it.next().unwrap_or("")})
        }).collect())
        .unwrap_or_default();
    json!({"Errors": errs, "Crashes": [], "Dumps": []})
}

// ------------------------------------------------------------- network ----

fn ping(host: &str) -> (bool, String) {
    let args: Vec<&str> = if cfg!(windows) { vec!["-n", "2", "-w", "1500", host] } else { vec!["-c", "2", "-W", "2", host] };
    match util::run_capture("ping", &args) {
        Ok((code, out)) => {
            let avg = out
                .lines()
                .find_map(|l| l.split("Average = ").nth(1).map(|s| s.trim().to_string()).or_else(|| l.split(" = ").nth(1).and_then(|s| s.split('/').nth(1)).map(|s| format!("{s}ms"))))
                .unwrap_or_default();
            (code == 0, avg)
        }
        Err(e) => (false, e.to_string()),
    }
}

pub fn network() -> Value {
    #[cfg(windows)]
    let adapters = util::powershell_json(r#"Get-NetIPConfiguration -ErrorAction SilentlyContinue | Where-Object { $_.NetAdapter.Status -eq 'Up' } | ForEach-Object { [pscustomobject]@{Name=$_.InterfaceAlias; Desc=$_.InterfaceDescription; IPv4=($_.IPv4Address.IPAddress -join ', '); Gateway=($_.IPv4DefaultGateway.NextHop -join ', '); DNS=($_.DNSServer.ServerAddresses -join ', '); Speed="$($_.NetAdapter.LinkSpeed)"; Mac=$_.NetAdapter.MacAddress} } | ConvertTo-Json -Compress"#).unwrap_or_default();
    #[cfg(not(windows))]
    let adapters: Vec<Value> = {
        let gw = util::run_capture("ip", &["route", "show", "default"]).map(|(_, o)| o.split_whitespace().nth(2).unwrap_or("").to_string()).unwrap_or_default();
        util::run_capture("ip", &["-j", "-4", "addr"])
            .ok()
            .and_then(|(_, o)| serde_json::from_str::<Vec<Value>>(&o).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|a| a["ifname"] != "lo")
            .map(|a| json!({"Name": a["ifname"], "Desc": "", "IPv4": a["addr_info"].as_array().map(|x| x.iter().filter_map(|i| i["local"].as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default(), "Gateway": gw, "DNS": "", "Speed": "", "Mac": a["address"]}))
            .collect()
    };
    let gateway = adapters.iter().find_map(|a| a["Gateway"].as_str().and_then(|g| g.split(',').next()).map(str::trim).filter(|g| !g.is_empty()).map(String::from));
    let (gw_ok, gw_ms) = gateway.as_deref().map(ping).unwrap_or((false, "no gateway".into()));
    let (net_ok, net_ms) = ping("1.1.1.1");
    let dns = std::net::ToSocketAddrs::to_socket_addrs(&("www.microsoft.com", 443)).map(|mut a| a.next().map(|x| x.ip().to_string()).unwrap_or_default());
    let tcp = std::net::TcpStream::connect_timeout(&"1.1.1.1:443".parse().unwrap(), std::time::Duration::from_secs(3)).is_ok();
    json!({
        "Adapters": adapters,
        "Tests": [
            {"Name": "Router / gateway", "Target": gateway.unwrap_or_default(), "Ok": gw_ok, "Detail": gw_ms},
            {"Name": "Internet (ping)", "Target": "1.1.1.1", "Ok": net_ok, "Detail": net_ms},
            {"Name": "DNS lookup", "Target": "www.microsoft.com", "Ok": dns.is_ok(), "Detail": dns.as_ref().map(|s| s.clone()).unwrap_or_else(|e| e.to_string())},
            {"Name": "HTTPS port", "Target": "1.1.1.1:443", "Ok": tcp, "Detail": if tcp { "open" } else { "blocked" }},
        ]
    })
}

/// Saved Wi-Fi networks and their keys (this PC's own profiles).
pub fn wifi() -> Value {
    #[cfg(windows)]
    {
        let Ok((_, out)) = util::run_capture("netsh", &["wlan", "show", "profiles"]) else { return json!([]) };
        let names: Vec<String> = out.lines().filter_map(|l| l.split_once(" : ").map(|(_, n)| n.trim().to_string())).filter(|n| !n.is_empty()).collect();
        let list: Vec<Value> = names
            .iter()
            .map(|n| {
                let arg = format!("name={n}");
                let detail = util::run_capture("netsh", &["wlan", "show", "profile", &arg, "key=clear"]).map(|(_, o)| o).unwrap_or_default();
                let field = |k: &str| detail.lines().find(|l| l.trim_start().starts_with(k)).and_then(|l| l.split_once(" : ")).map(|(_, v)| v.trim().to_string()).unwrap_or_default();
                json!({"Name": n, "Auth": field("Authentication"), "Key": field("Key Content")})
            })
            .collect();
        json!(list)
    }
    #[cfg(not(windows))]
    {
        let dir = std::path::Path::new("/etc/NetworkManager/system-connections");
        let list: Vec<Value> = std::fs::read_dir(dir)
            .map(|rd| rd.flatten().filter_map(|e| {
                let t = std::fs::read_to_string(e.path()).ok()?;
                let get = |k: &str| t.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).unwrap_or("").to_string();
                let ssid = get("ssid");
                (!ssid.is_empty()).then(|| json!({"Name": ssid, "Auth": get("key-mgmt"), "Key": get("psk")}))
            }).collect())
            .unwrap_or_default();
        json!(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_utf16_and_utf8() {
        let u16le: Vec<u8> = "Beginning system scan.\r\n".encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
        assert_eq!(decode(&u16le), "Beginning system scan.\r\n");
        assert_eq!(decode(b"plain"), "plain");
    }

    #[test]
    fn task_ids_unique() {
        let mut ids: Vec<_> = TASKS.iter().map(|t| t.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), TASKS.len());
    }
}
