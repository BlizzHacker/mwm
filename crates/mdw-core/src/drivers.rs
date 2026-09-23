//! Driver inventory. MDW does not download drivers from third-party mirrors
//! (that is how "driver updaters" ship malware); it shows what is installed,
//! flags old ones, and hands off to Windows Update / the vendor.

use serde::Serialize;

#[allow(unused_imports)]
use crate::util;

#[derive(Debug, Clone, Serialize)]
pub struct Driver {
    pub device: String,
    pub class: String,
    pub provider: String,
    pub version: String,
    pub date: String,
    pub inf: String,
    pub age_years: f32,
}

#[cfg(windows)]
pub fn list() -> Vec<Driver> {
    let script = "Get-CimInstance Win32_PnPSignedDriver | Where-Object { $_.DeviceName -and $_.DriverProviderName -and $_.DriverProviderName -ne 'Microsoft' } | Select-Object DeviceName,DeviceClass,DriverProviderName,DriverVersion,@{n='Date';e={ if ($_.DriverDate) { $_.DriverDate.ToString('yyyy-MM-dd') } }},InfName | ConvertTo-Json -Compress";
    let rows = util::powershell_json(script).unwrap_or_default();
    let now_year = 1970.0 + util::now_secs() as f32 / 31_557_600.0;
    let mut v: Vec<Driver> = rows
        .iter()
        .map(|r| {
            let date = util::js_str(r, "Date");
            let age = date
                .split('-')
                .collect::<Vec<_>>()
                .as_slice()
                .iter()
                .take(2)
                .map(|x| x.parse::<f32>().unwrap_or(0.0))
                .collect::<Vec<_>>();
            let age_years = if age.len() == 2 && age[0] > 1990.0 { (now_year - (age[0] + (age[1] - 1.0) / 12.0)).max(0.0) } else { 0.0 };
            Driver {
                device: util::js_str(r, "DeviceName"),
                class: util::js_str(r, "DeviceClass"),
                provider: util::js_str(r, "DriverProviderName"),
                version: util::js_str(r, "DriverVersion"),
                date,
                inf: util::js_str(r, "InfName"),
                age_years,
            }
        })
        .collect();
    v.sort_by(|a, b| b.age_years.partial_cmp(&a.age_years).unwrap_or(std::cmp::Ordering::Equal));
    v
}

#[cfg(target_os = "linux")]
pub fn list() -> Vec<Driver> {
    let Ok(rd) = std::fs::read_dir("/sys/module") else { return vec![] };
    let mut v: Vec<Driver> = rd
        .flatten()
        .filter(|e| e.path().join("initstate").exists())
        .map(|e| {
            let version = std::fs::read_to_string(e.path().join("version")).unwrap_or_default().trim().to_string();
            Driver {
                device: e.file_name().to_string_lossy().into(),
                class: "kernel module".into(),
                provider: String::new(),
                version,
                date: String::new(),
                inf: String::new(),
                age_years: 0.0,
            }
        })
        .collect();
    v.sort_by(|a, b| a.device.cmp(&b.device));
    v
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn list() -> Vec<Driver> {
    vec![]
}
