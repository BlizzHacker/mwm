//! Machine facts for the dashboard, elevation, and small OS helpers.

use serde::Serialize;
use sysinfo::{Disks, System};

#[derive(Debug, Clone, Serialize)]
pub struct DiskInfo {
    pub mount: String,
    pub name: String,
    pub fs: String,
    pub total: u64,
    pub free: u64,
    pub removable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemInfo {
    pub os: String,
    pub platform: String,
    pub hostname: String,
    pub elevated: bool,
    pub version: String,
    pub memory_total: u64,
    pub memory_used: u64,
    pub cpu: String,
    pub cores: usize,
    pub uptime_secs: u64,
    pub disks: Vec<DiskInfo>,
}

pub fn disks() -> Vec<DiskInfo> {
    let mut v: Vec<DiskInfo> = Disks::new_with_refreshed_list()
        .list()
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|d| DiskInfo {
            mount: d.mount_point().to_string_lossy().into(),
            name: d.name().to_string_lossy().into(),
            fs: d.file_system().to_string_lossy().into(),
            total: d.total_space(),
            free: d.available_space(),
            removable: d.is_removable(),
        })
        .filter(|d| !matches!(d.fs.as_str(), "overlay" | "tmpfs" | "squashfs" | "devtmpfs"))
        .collect();
    v.sort_by(|a, b| a.mount.cmp(&b.mount));
    v.dedup_by(|a, b| a.name == b.name && a.total == b.total && !a.name.is_empty());
    v
}

pub fn info() -> SystemInfo {
    let mut s = System::new();
    s.refresh_memory();
    s.refresh_cpu_list(sysinfo::CpuRefreshKind::nothing());
    SystemInfo {
        os: System::long_os_version().unwrap_or_else(|| std::env::consts::OS.into()),
        platform: std::env::consts::OS.into(),
        hostname: System::host_name().unwrap_or_default(),
        elevated: is_elevated(),
        version: crate::VERSION.into(),
        memory_total: s.total_memory(),
        memory_used: s.used_memory(),
        cpu: s.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default(),
        cores: s.cpus().len(),
        uptime_secs: System::uptime(),
        disks: disks(),
    }
}

#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut h: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut h) == 0 {
            return false;
        }
        let mut e = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut len = 0u32;
        let ok = GetTokenInformation(
            h,
            TokenElevation,
            &mut e as *mut _ as *mut core::ffi::c_void,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        );
        CloseHandle(h);
        ok != 0 && e.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("Uid:")).and_then(|l| l.split_whitespace().nth(2)).map(|u| u == "0"))
        .unwrap_or_else(|| crate::util::run_capture("id", &["-u"]).map(|(_, o)| o.trim() == "0").unwrap_or(false))
}

/// Start this same executable again with administrator rights (UAC prompt).
#[cfg(windows)]
pub fn relaunch_elevated(args: &str) -> anyhow::Result<()> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    let exe = std::env::current_exe()?;
    let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (verb, file, params) = (w("runas"), w(&exe.to_string_lossy()), w(args));
    let r = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), params.as_ptr(), std::ptr::null(), 1) };
    if (r as isize) <= 32 {
        anyhow::bail!("Administrator restart was cancelled.");
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn relaunch_elevated(_args: &str) -> anyhow::Result<()> {
    anyhow::bail!("Run MWM with sudo / pkexec for system-wide cleaning.")
}

/// Show a file or folder in the system file manager.
pub fn reveal(path: &str) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        let p = std::path::Path::new(path);
        let arg = if p.is_file() { format!("/select,\"{path}\"") } else { format!("\"{path}\"") };
        // explorer.exe parses its own command line; pass it raw.
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer.exe").raw_arg(arg).spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        crate::util::cmd("open").args(["-R", path]).spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let p = std::path::Path::new(path);
        let dir = if p.is_file() { p.parent().unwrap_or(p) } else { p };
        crate::util::cmd("xdg-open").arg(dir).spawn()?;
    }
    Ok(())
}

/// Open a file/folder with its default app (non-blocking).
pub fn open_default(path: &str) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let (verb, file) = (w("open"), w(path));
        let r = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), 1) };
        if (r as isize) <= 32 {
            anyhow::bail!("no app is associated with this file");
        }
    }
    #[cfg(target_os = "macos")]
    crate::util::cmd("open").arg(path).spawn()?;
    #[cfg(all(unix, not(target_os = "macos")))]
    crate::util::cmd("xdg-open").arg(path).spawn()?;
    Ok(())
}

/// Open a plain-text editor on a file (Commander F4).
pub fn edit(path: &str) -> anyhow::Result<()> {
    #[cfg(windows)]
    std::process::Command::new("notepad.exe").arg(path).spawn()?;
    #[cfg(not(windows))]
    open_default(path)?;
    Ok(())
}

/// A terminal window in `dir`.
pub fn terminal(dir: &str) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let (verb, d) = (w("open"), w(dir));
        // Windows Terminal if present, else a classic console.
        let wt = w("wt.exe");
        let args = w(&format!("-d \"{dir}\""));
        let r = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), wt.as_ptr(), args.as_ptr(), d.as_ptr(), 1) };
        if (r as isize) <= 32 {
            let cmd = w("cmd.exe");
            unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), cmd.as_ptr(), std::ptr::null(), d.as_ptr(), 1) };
        }
    }
    #[cfg(target_os = "macos")]
    crate::util::cmd("open").args(["-a", "Terminal", dir]).spawn()?;
    #[cfg(all(unix, not(target_os = "macos")))]
    crate::util::cmd("x-terminal-emulator").current_dir(dir).spawn()?;
    Ok(())
}

/// Built-in OS tools (Revo's "Windows Tools" panel).
pub fn launch_tool(tool: &str) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        let (prog, args): (&str, Vec<&str>) = match tool {
            "cleanmgr" => ("cleanmgr.exe", vec![]),
            "restore" => ("rstrui.exe", vec![]),
            "devmgmt" => ("mmc.exe", vec!["devmgmt.msc"]),
            "services" => ("mmc.exe", vec!["services.msc"]),
            "eventvwr" => ("mmc.exe", vec!["eventvwr.msc"]),
            "taskschd" => ("mmc.exe", vec!["taskschd.msc"]),
            "diskmgmt" => ("mmc.exe", vec!["diskmgmt.msc"]),
            "msinfo" => ("msinfo32.exe", vec![]),
            "resmon" => ("resmon.exe", vec![]),
            "taskmgr" => ("taskmgr.exe", vec![]),
            "sysprops" => ("SystemPropertiesAdvanced.exe", vec![]),
            "regedit" => ("regedit.exe", vec![]),
            "winupdate" => ("explorer.exe", vec!["ms-settings:windowsupdate"]),
            "optupdates" => ("explorer.exe", vec!["ms-settings:windowsupdate-optionalupdates"]),
            "storage" => ("explorer.exe", vec!["ms-settings:storagesense"]),
            "apps" => ("explorer.exe", vec!["ms-settings:appsfeatures"]),
            "defrag" => ("dfrgui.exe", vec![]),
            other => anyhow::bail!("unknown tool {other}"),
        };
        // ShellExecute so tools that need elevation get a UAC prompt.
        let file = prog.to_string();
        let params = args.join(" ");
        std::thread::spawn(move || {
            let _ = crate::apps::shell_run_wait(&file, &params, None);
        });
        Ok(())
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("{tool}: system tools panel is Windows-only for now")
    }
}
