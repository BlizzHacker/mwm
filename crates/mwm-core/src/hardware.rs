//! Hardware inspection - the CPU-Z / GPU-Z replacement, built into MWM.
//!
//! One API on every OS: CPU identity/topology/speed, GPU list with VRAM and
//! driver, and live sensors (temperature, fan, load) where the platform
//! exposes them. Windows uses WMI; Linux reads sysfs (+ coretemp/k10temp);
//! macOS uses system_profiler. Values that a platform cannot supply are
//! null - never faked.

use serde_json::{json, Value};

use crate::util;

// ------------------------------------------------------------- common ----

pub fn overview() -> Value {
    let mut v = json!({
        "cpu": cpu(),
        "gpu": gpu(),
        "sensors": sensors(),
        "memory": memory(),
        "platform": if cfg!(windows) { "windows" } else if cfg!(target_os = "macos") { "macos" } else { "linux" },
    });
    v["motherboard"] = motherboard();
    v
}

fn round1(f: f64) -> f64 {
    (f * 10.0).round() / 10.0
}

// ------------------------------------------------------------ Windows ----

#[cfg(windows)]
fn cpu() -> Value {
    let rows = util::powershell_json("Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors,MaxClockSpeed,CurrentClockSpeed,LoadPercentage,Manufacturer,SocketDesignation | ConvertTo-Json -Compress").unwrap_or_default();
    rows.first().map(|c| json!({
        "name": c["Name"].as_str().unwrap_or("").trim(),
        "cores": c["NumberOfCores"],
        "threads": c["NumberOfLogicalProcessors"],
        "max_mhz": c["MaxClockSpeed"],
        "current_mhz": c["CurrentClockSpeed"],
        "load_percent": c["LoadPercentage"],
        "vendor": c["Manufacturer"],
        "socket": c["SocketDesignation"],
    })).unwrap_or(json!({"name": ""}))
}

#[cfg(windows)]
fn gpu() -> Value {
    let rows = util::powershell_json("Get-CimInstance Win32_VideoController | Select-Object Name,AdapterRAM,DriverVersion,DriverDate,CurrentHorizontalResolution,CurrentVerticalResolution,CurrentRefreshRate,VideoProcessor | ConvertTo-Json -Compress").unwrap_or_default();
    let nvidia = util::powershell_json("& nvidia-smi --query-gpu=name,memory.total,temperature.gpu,utilization.gpu,clocks.sm,power.draw --format=csv,noheader,nounits 2>$null | ConvertTo-Json -Compress").unwrap_or_default();
    let mut gpus: Vec<Value> = rows.iter().map(|g| json!({
        "name": g["Name"].as_str().unwrap_or(""),
        // AdapterRAM is 32-bit in WMI: caps at 4 GiB, null above that.
        "vram_bytes": g["AdapterRAM"].as_u64().filter(|v| *v > 0 && *v < 4_200_000_000),
        "driver": g["DriverVersion"],
        "driver_date": g["DriverDate"],
        "resolution": format!("{}x{}@{}Hz",
            g["CurrentHorizontalResolution"].as_u64().unwrap_or(0),
            g["CurrentVerticalResolution"].as_u64().unwrap_or(0),
            g["CurrentRefreshRate"].as_u64().unwrap_or(0)),
    })).collect();
    // nvidia-smi gives the real telemetry WMI cannot.
    for row in &nvidia {
        if let Some(line) = row.as_str() {
            let f: Vec<&str> = line.split(',').map(str::trim).collect();
            if f.len() >= 6 {
                if let Some(g) = gpus.iter_mut().find(|g| f[0].contains(g["name"].as_str().unwrap_or("~none~"))) {
                    g["vram_mb"] = json!(f[1]);
                    g["temp_c"] = json!(f[2]);
                    g["util_percent"] = json!(f[3]);
                    g["clock_mhz"] = json!(f[4]);
                    g["power_w"] = json!(f[5]);
                } else {
                    gpus.push(json!({"name": f[0], "vram_mb": f[1], "temp_c": f[2], "util_percent": f[3], "clock_mhz": f[4], "power_w": f[5], "source": "nvidia-smi"}));
                }
            }
        }
    }
    json!(gpus)
}

#[cfg(windows)]
fn sensors() -> Value {
    // Thermal zone (works on most laptops/desktops with ACPI thermal zones).
    let tz = util::powershell_json("Get-CimInstance -Namespace root/wmi -ClassName MSAcpi_ThermalZoneTemperature -ErrorAction SilentlyContinue | Select-Object InstanceName,CurrentTemperature | ConvertTo-Json -Compress").unwrap_or_default();
    let temps: Vec<Value> = tz.iter().filter_map(|t| {
        let deci_k = t["CurrentTemperature"].as_f64()?;
        Some(json!({"name": t["InstanceName"].as_str().unwrap_or("thermal zone"), "temp_c": round1(deci_k / 10.0 - 273.15)}))
    }).collect();
    json!({"temps": temps, "note": if temps.is_empty() { "No ACPI thermal zones exposed; install Libre Hardware Monitor for full sensor coverage (Rescue catalog)." } else { "" }})
}

#[cfg(windows)]
fn memory() -> Value {
    let rows = util::powershell_json("Get-CimInstance Win32_PhysicalMemory | Select-Object Capacity,Speed,Manufacturer,PartNumber,DeviceLocator,ConfiguredClockSpeed | ConvertTo-Json -Compress").unwrap_or_default();
    let sticks: Vec<Value> = rows.iter().map(|m| json!({
        "slot": m["DeviceLocator"], "size_bytes": m["Capacity"], "speed_mts": m["Speed"],
        "configured_mts": m["ConfiguredClockSpeed"], "vendor": m["Manufacturer"], "part": m["PartNumber"],
    })).collect();
    json!({"sticks": sticks})
}

#[cfg(windows)]
fn motherboard() -> Value {
    let b = util::powershell_json("Get-CimInstance Win32_BaseBoard | Select-Object Manufacturer,Product,Version,SerialNumber | ConvertTo-Json -Compress").unwrap_or_default();
    let s = util::powershell_json("Get-CimInstance Win32_BIOS | Select-Object Manufacturer,SMBIOSBIOSVersion,ReleaseDate | ConvertTo-Json -Compress").unwrap_or_default();
    json!({
        "manufacturer": b.first().map(|x| x["Manufacturer"].clone()).unwrap_or(Value::Null),
        "product": b.first().map(|x| x["Product"].clone()).unwrap_or(Value::Null),
        "bios_vendor": s.first().map(|x| x["Manufacturer"].clone()).unwrap_or(Value::Null),
        "bios_version": s.first().map(|x| x["SMBIOSBIOSVersion"].clone()).unwrap_or(Value::Null),
    })
}

// -------------------------------------------------------------- Linux ----

#[cfg(not(windows))]
fn read(p: &str) -> String {
    std::fs::read_to_string(p).unwrap_or_default().trim().to_string()
}

#[cfg(not(windows))]
fn cpu() -> Value {
    let info = read("/proc/cpuinfo");
    let model = info.lines().find_map(|l| l.strip_prefix("model name").and_then(|v| v.split(':').nth(1))).map(str::trim).unwrap_or("");
    let cores = info.lines().filter(|l| l.starts_with("processor")).count();
    let physical = {
        let mut ids: Vec<&str> = info.lines().filter_map(|l| l.strip_prefix("core id").and_then(|v| v.split(':').nth(1))).collect();
        ids.sort(); ids.dedup(); ids.len()
    };
    let mhz = info.lines().find_map(|l| l.strip_prefix("cpu MHz").and_then(|v| v.split(':').nth(1))).and_then(|v| v.trim().parse::<f64>().ok());
    let stat = read("/proc/stat");
    json!({
        "name": if model.is_empty() { Value::Null } else { json!(model) },
        "cores": if physical > 0 { json!(physical) } else { Value::Null },
        "threads": if cores > 0 { json!(cores) } else { Value::Null },
        "current_mhz": mhz.map(|m| json!(m.round())).unwrap_or(Value::Null),
        "load_percent": load_percent(&stat),
    })
}

#[cfg(not(windows))]
fn load_percent(_stat: &str) -> Value {
    // 1-minute loadavg vs thread count, like top's %.
    match read("/proc/loadavg").split_whitespace().next().and_then(|s| s.parse::<f64>().ok()) {
        Some(l) => {
            let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
            json!(round1(l / threads as f64 * 100.0))
        }
        None => Value::Null,
    }
}

#[cfg(not(windows))]
fn gpu() -> Value {
    // lspci is universal; sysfs DRM adds VRAM/driver where present.
    let (code, out) = util::run_capture("lspci", &[]).unwrap_or((1, String::new()));
    let mut gpus = Vec::new();
    if code == 0 {
        for line in out.lines() {
            // "06:00.0 VGA compatible controller: Matrox ... MGA G200e (rev 02)"
            if line.contains("VGA compatible controller") || line.contains("3D controller") || line.contains("Display controller") {
                let name = line.splitn(2, ": ").nth(1)
                    .map(|n| n.split(" (rev").next().unwrap_or(n).trim())
                    .unwrap_or(line);
                gpus.push(json!({"name": name, "source": "lspci"}));
            }
        }
    }
    // NVIDIA telemetry when the driver is loaded.
    if let Ok((0, out)) = util::run_capture("nvidia-smi", &["--query-gpu=name,memory.total,memory.used,temperature.gpu,utilization.gpu,clocks.sm,power.draw,driver_version", "--format=csv,noheader,nounits"]) {
        gpus.clear();
        for line in out.lines() {
            let f: Vec<&str> = line.split(',').map(str::trim).collect();
            if f.len() >= 8 {
                gpus.push(json!({"name": f[0], "vram_mb": f[1], "vram_used_mb": f[2], "temp_c": f[3], "util_percent": f[4], "clock_mhz": f[5], "power_w": f[6], "driver": f[7], "source": "nvidia-smi"}));
            }
        }
    }
    // AMD GPU sysfs telemetry.
    for card in 0..4 {
        let base = format!("/sys/class/drm/card{card}/device");
        if !std::path::Path::new(&base).exists() { continue; }
        let vram = read(&format!("{base}/mem_info_vram_total")).parse::<u64>().ok();
        let used = read(&format!("{base}/mem_info_vram_used")).parse::<u64>().ok();
        if vram.is_some() || used.is_some() {
            // attach to the matching lspci entry if we have one
            let idx = card.min(gpus.len().saturating_sub(1));
            if !gpus.is_empty() && idx < gpus.len() {
              if let Some(g) = gpus.get_mut(idx) {
                if let Some(v) = vram { g["vram_bytes"] = json!(v); }
                if let Some(u) = used { g["vram_used_bytes"] = json!(u); }
                g["source"] = json!("sysfs-amdgpu");
              }
            } else if vram.is_some() {
                gpus.push(json!({"name": format!("card{card} (amdgpu)"), "vram_bytes": vram, "vram_used_bytes": used, "source": "sysfs-amdgpu"}));
            }
        }
    }
    json!(gpus)
}

#[cfg(not(windows))]
fn sensors() -> Value {
    let mut temps: Vec<Value> = Vec::new();
    for zone in 0..8 {
        let t = read(&format!("/sys/class/thermal/thermal_zone{zone}/temp")).parse::<i64>().ok();
        let typ = read(&format!("/sys/class/thermal/thermal_zone{zone}/type"));
        if let Some(milli) = t {
            temps.push(json!({"name": if typ.is_empty() { format!("zone{zone}") } else { typ }, "temp_c": round1(milli as f64 / 1000.0)}));
        }
    }
    // coretemp/k10temp hwmon entries.
    let mut fans: Vec<Value> = Vec::new();
    if let Ok(rd) = std::fs::read_dir("/sys/class/hwmon") {
        for h in rd.flatten() {
            let chip = read(&h.path().join("name").to_string_lossy());
            for i in 1..8 {
                let p = h.path().join(format!("temp{i}_input"));
                if let Ok(milli) = std::fs::read_to_string(&p) {
                    if let Ok(v) = milli.trim().parse::<i64>() {
                        let label = read(&h.path().join(format!("temp{i}_label")).to_string_lossy());
                        temps.push(json!({"name": format!("{chip}{}", if label.is_empty() { String::new() } else { format!(" {label}") }), "temp_c": round1(v as f64 / 1000.0)}));
                    }
                }
                let fp = h.path().join(format!("fan{i}_input"));
                if let Ok(rpm) = std::fs::read_to_string(&fp) {
                    if let Ok(v) = rpm.trim().parse::<u64>() {
                        fans.push(json!({"name": format!("{chip} fan{i}"), "rpm": v}));
                    }
                }
            }
        }
    }
    json!({"temps": temps, "fans": fans})
}

#[cfg(not(windows))]
fn memory() -> Value {
    let dmi = util::run_capture("dmidecode", &["-t", "memory"]).map(|(_, o)| o).unwrap_or_default();
    let mut sticks = Vec::new();
    let mut cur: Option<Value> = None;
    for line in dmi.lines() {
        let l = line.trim();
        if l == "Memory Device" {
            if let Some(c) = cur.take() { if c["size_bytes"] != Value::Null { sticks.push(c); } }
            cur = Some(json!({"size_bytes": Value::Null, "speed_mts": Value::Null, "vendor": Value::Null, "part": Value::Null, "slot": Value::Null}));
        }
        if let Some(c) = cur.as_mut() {
            if let Some(v) = l.strip_prefix("Size:") {
                let v = v.trim();
                if !v.starts_with("No Module") {
                    let mb: u64 = v.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(0);
                    let unit_gb = v.contains("GB");
                    c["size_bytes"] = json!(mb * if unit_gb { 1 << 30 } else { 1 << 20 });
                }
            } else if let Some(v) = l.strip_prefix("Speed:") {
                c["speed_mts"] = json!(v.trim().split_whitespace().next().and_then(|n| n.parse::<u64>().ok()));
            } else if let Some(v) = l.strip_prefix("Manufacturer:") {
                let v = v.trim(); if !v.is_empty() && v != "Unknown" && v != "Not Specified" { c["vendor"] = json!(v); }
            } else if let Some(v) = l.strip_prefix("Part Number:") {
                let v = v.trim(); if !v.is_empty() && v != "Unknown" && v != "Not Specified" { c["part"] = json!(v); }
            } else if let Some(v) = l.strip_prefix("Locator:") {
                let v = v.trim(); if !v.is_empty() { c["slot"] = json!(v); }
            }
        }
    }
    if let Some(c) = cur { if c["size_bytes"] != Value::Null { sticks.push(c); } }
    json!({"sticks": sticks, "note": if sticks.is_empty() { "dmidecode unavailable or no DMI data (VM?)." } else { "" }})
}

#[cfg(not(windows))]
fn motherboard() -> Value {
    json!({
        "manufacturer": opt(read("/sys/class/dmi/id/board_vendor")),
        "product": opt(read("/sys/class/dmi/id/board_name")),
        "bios_vendor": opt(read("/sys/class/dmi/id/bios_vendor")),
        "bios_version": opt(read("/sys/class/dmi/id/bios_version")),
    })
}

#[cfg(not(windows))]
fn opt(s: String) -> Value {
    if s.is_empty() { Value::Null } else { json!(s) }
}
