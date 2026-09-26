//! One command table for every front end: the desktop app (Tauri `invoke`)
//! and `mwm serve` (HTTP) both call `call(cmd, args)`. Argument names are the
//! camelCase keys the UI sends.

use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::{apps, cleaner, disk, drivers, dupes, files, jobs, keys, procs, remote, server, shred, startup, sys, toolkit, updater};

fn arg<T: DeserializeOwned>(a: &Value, k: &str) -> Result<T, String> {
    serde_json::from_value(a.get(k).cloned().unwrap_or(Value::Null)).map_err(|e| format!("argument `{k}`: {e}"))
}

fn ok<T: serde::Serialize>(v: T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

fn e<T: serde::Serialize>(r: anyhow::Result<T>) -> Result<Value, String> {
    r.map_err(|x| x.to_string()).and_then(ok)
}

/// Commands that change the machine. `mwm serve --read-only` refuses them.
pub fn is_mutating(cmd: &str) -> bool {
    matches!(
        cmd,
        "cleaner_clean" | "app_uninstall" | "leftovers_remove" | "startup_set" | "service_mode" | "update_apply" | "dupes_remove"
            | "shred_paths" | "wipe_free" | "procs_kill" | "files_mkdir" | "files_rename" | "files_transfer" | "files_delete"
            | "files_pack" | "files_unpack" | "files_multi_rename" | "files_write" | "toolkit_run" | "server_action"
            | "conn_save" | "conn_remove" | "guest_docker_action" | "deploy_node" | "pve_op"
            | "lab_quarantine" | "lab_quarantine_restore" | "lab_quarantine_delete"
            | "arkana_configure" | "arkana_connect_lxc" | "arkana_install" | "arkana_control" | "arkana_tool" | "arkana_analyze"
            | "arr_mcp_configure" | "arr_mcp_call"
            | "lxc_fs_write" | "lxc_fs_mkdir" | "lxc_fs_delete"
            | "vault_store"
    )
}

/// Like `is_mutating`, but looks inside `remote_call` at the proxied command.
pub fn is_mutating_call(cmd: &str, args: &Value) -> bool {
    if cmd == "remote_call" {
        return args.get("cmd").and_then(|c| c.as_str()).map(is_mutating).unwrap_or(true);
    }
    is_mutating(cmd)
}

pub fn call(cmd: &str, a: &Value) -> Result<Value, String> {
    match cmd {
        "system_info" => ok(sys::info()),
        "cleaner_scan" => ok(cleaner::scan_all()),
        "cleaner_clean" => ok(cleaner::clean(&arg::<Vec<String>>(a, "ids")?)),
        "apps_list" => ok(apps::list(arg::<Option<bool>>(a, "store")?.unwrap_or(false))),
        "app_uninstall" => ok(apps::uninstall(&arg(a, "app")?, arg::<Option<bool>>(a, "quiet")?.unwrap_or(false))),
        "app_leftovers" => ok(apps::leftovers(&arg(a, "app")?)),
        "leftovers_remove" => ok(apps::remove_leftovers(&arg::<Vec<apps::Leftover>>(a, "items")?)),
        "startup_list" => ok(startup::list()),
        "startup_set" => ok(startup::set_enabled(&arg::<String>(a, "id")?, arg(a, "enabled")?)),
        "service_mode" => ok(startup::set_service_mode(&arg::<String>(a, "name")?, &arg::<String>(a, "mode")?)),
        "updates_list" => e(updater::list()),
        "update_apply" => ok(updater::apply(&arg::<String>(a, "id")?)),
        "dupes_find" => {
            let roots: Vec<PathBuf> = arg::<Vec<String>>(a, "roots")?.into_iter().map(PathBuf::from).collect();
            ok(dupes::find(&roots, arg::<Option<u64>>(a, "minSize")?.unwrap_or(1), false))
        }
        "dupes_remove" => {
            let (removed, failed) = dupes::remove(&arg::<Vec<String>>(a, "paths")?);
            ok(json!({ "removed": removed, "failed": failed }))
        }
        "disk_analyze" => ok(disk::analyze(&PathBuf::from(arg::<String>(a, "root")?), 60)),
        "shred_paths" => ok(shred::shred(&arg::<Vec<String>>(a, "paths")?, arg::<Option<u32>>(a, "passes")?.unwrap_or(1))),
        "wipe_free" => e(shred::wipe_free_space(&arg::<String>(a, "dir")?)),
        "drivers_list" => ok(drivers::list()),
        "procs_list" => ok(procs::list()),
        "procs_kill" => ok(procs::kill(&arg::<Vec<u32>>(a, "pids")?)),
        "reveal" => e(sys::reveal(&arg::<String>(a, "path")?)),
        "launch_tool" => e(sys::launch_tool(&arg::<String>(a, "tool")?)),
        // Commander
        "files_roots" => ok(files::roots()),
        "files_list" => e(files::list(&arg::<String>(a, "dir")?)),
        "files_mkdir" => e(files::mkdir(&arg::<String>(a, "parent")?, &arg::<String>(a, "name")?)),
        "files_rename" => e(files::rename(&arg::<String>(a, "path")?, &arg::<String>(a, "name")?)),
        "files_conflicts" => ok(files::conflicts(&arg::<Vec<String>>(a, "sources")?, &arg::<String>(a, "dest")?)),
        "files_transfer" => ok(files::transfer(arg(a, "sources")?, arg(a, "dest")?, arg::<Option<String>>(a, "mode")?.unwrap_or_else(|| "skip".into()), arg::<Option<bool>>(a, "isMove")?.unwrap_or(false))),
        "files_delete" => ok(files::delete(arg(a, "paths")?, arg::<Option<bool>>(a, "permanent")?.unwrap_or(false))),
        "files_pack" => ok(files::pack(arg(a, "sources")?, arg(a, "archive")?)),
        "files_unpack" => ok(files::unpack(arg(a, "archive")?, arg(a, "dest")?)),
        "files_search" => ok(files::search(arg(a, "root")?, arg::<Option<String>>(a, "pattern")?.unwrap_or_default(), arg::<Option<String>>(a, "text")?.unwrap_or_default())),
        "files_preview" => e(files::preview(&arg::<String>(a, "path")?)),
        "files_props" => e(files::properties(&arg::<String>(a, "path")?)),
        "files_dir_sizes" => ok(files::dir_sizes(&arg::<Vec<String>>(a, "paths")?)),
        "files_multi_rename" => e(files::multi_rename(&arg::<Vec<(String, String)>>(a, "plan")?)),
        "files_write" => e(files::write_text(&arg::<String>(a, "path")?, &arg::<String>(a, "text")?)),
        "open_default" => e(sys::open_default(&arg::<String>(a, "path")?)),
        "edit_file" => e(sys::edit(&arg::<String>(a, "path")?)),
        "terminal" => e(sys::terminal(&arg::<String>(a, "dir")?)),
        // Jobs
        "jobs_list" => ok(jobs::list()),
        "job_cancel" => {
            jobs::cancel(arg(a, "id")?);
            ok(true)
        }
        "jobs_clear" => {
            jobs::clear_finished();
            ok(true)
        }
        // Toolkit
        "toolkit_tasks" => ok(toolkit::TASKS.to_vec()),
        "toolkit_run" => e(toolkit::run_task(&arg::<String>(a, "id")?)),
        "toolkit_report" => ok(toolkit::report()),
        "toolkit_security" => ok(toolkit::security()),
        "toolkit_events" => ok(toolkit::events()),
        "toolkit_network" => ok(toolkit::network()),
        "toolkit_wifi" => ok(toolkit::wifi()),
        "keys_list" => ok(keys::list()),
        "vault_load" => e(crate::vault::load()),
        "vault_store" => e(crate::vault::store(&arg::<String>(a, "expectedRevision")?, a.get("blob").ok_or("missing encrypted vault blob")?)),
        // Offline malware triage and the user's own Arkana instance.
        "lab_analyze" => e(crate::lab::analyze(&arg::<String>(a, "path")?)),
        "lab_av_scan" => e(crate::lab::av_scan(&arg::<String>(a, "path")?)),
        "lab_quarantine" => e(crate::lab::quarantine(&arg::<String>(a, "path")?, &arg::<Option<String>>(a, "reason")?.unwrap_or_default())),
        "lab_quarantine_list" => ok(crate::lab::quarantine_list()),
        "lab_quarantine_restore" => e(crate::lab::quarantine_restore(&arg::<String>(a, "id")?)),
        "lab_quarantine_delete" => e(crate::lab::quarantine_delete(&arg::<String>(a, "id")?)),
        "arkana_status" => ok(crate::arkana::status()),
        "arkana_configure" => e(crate::arkana::configure(&arg::<String>(a, "url")?, &arg::<Option<String>>(a, "key")?.unwrap_or_default(), &arg::<Option<String>>(a, "samplesDir")?.unwrap_or_default())),
        "arkana_connect_lxc" => e(crate::arkana::connect_lxc(&arg::<String>(a, "vmid")?)),
        "arkana_install" => ok(crate::arkana::install(&arg::<Option<String>>(a, "repo")?.unwrap_or_default(), &arg::<Option<String>>(a, "dir")?.unwrap_or_default(), arg::<Option<bool>>(a, "lan")?.unwrap_or(false))),
        "arkana_control" => e(crate::arkana::control(&arg::<String>(a, "action")?)),
        "arkana_tools" => e(crate::arkana::tools()),
        "arkana_tool" => e(crate::arkana::tool(&arg::<String>(a, "name")?, a.get("args").unwrap_or(&Value::Null))),
        "arkana_analyze" => e(crate::arkana::analyze(&arg::<String>(a, "path")?)),
        "plugins_list" => e(crate::plugins::list()),
        "arr_mcp_status" => ok(crate::mcp_arr::status()),
        "arr_mcp_configure" => e(crate::mcp_arr::configure(&arg::<String>(a, "url")?)),
        "arr_mcp_tools" => e(crate::mcp_arr::tools()),
        "arr_mcp_call" => e(crate::mcp_arr::call(&arg::<String>(a, "name")?, a.get("args").unwrap_or(&Value::Null))),
        // Servers (Proxmox / Unraid / ZFS / Docker / SMART)
        "server_info" => e(server::info_on(&arg::<Option<String>>(a, "node")?.unwrap_or_default())),
        "pve_overview" => e(crate::pve::overview()),
        "pve_guest" => e(crate::pve::guest(&arg::<String>(a, "node")?, &arg::<String>(a, "kind")?, &arg::<String>(a, "vmid")?)),
        "pve_op" => e(crate::pve::guest_op(&arg::<String>(a, "node")?, &arg::<String>(a, "kind")?, &arg::<String>(a, "vmid")?, &arg::<String>(a, "op")?, a.get("params").unwrap_or(&Value::Null))),
        "pve_task_log" => e(crate::pve::task_log(&arg::<String>(a, "node")?, &arg::<String>(a, "upid")?)),
        "lxc_fs_list" => e(crate::lxc_fs::list(&arg::<String>(a, "node")?, &arg::<String>(a, "vmid")?, &arg::<String>(a, "dir")?)),
        "lxc_fs_read" => e(crate::lxc_fs::read(&arg::<String>(a, "node")?, &arg::<String>(a, "vmid")?, &arg::<String>(a, "path")?)),
        "lxc_fs_write" => e(crate::lxc_fs::write(&arg::<String>(a, "node")?, &arg::<String>(a, "vmid")?, &arg::<String>(a, "path")?, &arg::<String>(a, "data")?)),
        "lxc_fs_mkdir" => e(crate::lxc_fs::mkdir(&arg::<String>(a, "node")?, &arg::<String>(a, "vmid")?, &arg::<String>(a, "path")?)),
        "lxc_fs_delete" => e(crate::lxc_fs::delete(&arg::<String>(a, "node")?, &arg::<String>(a, "vmid")?, &arg::<String>(a, "path")?)),
        "deploy_node" => e(server::deploy_node(&arg::<String>(a, "node")?)),
        "server_action" => e(server::action(&arg::<String>(a, "action")?, &arg::<Option<String>>(a, "target")?.unwrap_or_default())),
        "guest_docker" => e(server::guest_docker(&arg::<String>(a, "node")?, &arg::<String>(a, "vmid")?)),
        "guest_docker_action" => e(server::guest_docker_action(&arg::<String>(a, "node")?, &arg::<String>(a, "vmid")?, &arg::<String>(a, "action")?, &arg::<Option<String>>(a, "target")?.unwrap_or_default())),
        "docker_scan_cluster" => ok(server::docker_scan_cluster()),
        // This machine's own server (desktop "Remote access" switch).
        "remote_access_status" => ok(crate::web::ra_status()),
        "remote_access_start" => e(crate::web::ra_start(&arg::<Option<String>>(a, "bind")?.unwrap_or_else(|| "0.0.0.0:7777".into()), arg::<Option<bool>>(a, "readOnly")?.unwrap_or(false))),
        "remote_access_stop" => e(crate::web::ra_stop()),
        "remote_access_new_token" => ok(crate::web::ra_new_token()),
        // Other machines
        "conn_list" => ok(remote::list()),
        "conn_save" => e(remote::save(arg(a, "id")?, &arg::<String>(a, "name")?, &arg::<String>(a, "url")?, &arg::<Option<String>>(a, "token")?.unwrap_or_default())),
        "conn_remove" => e(remote::remove(&arg::<String>(a, "id")?)),
        "remote_call" => remote::call(&arg::<String>(a, "id")?, &arg::<String>(a, "cmd")?, a.get("args").unwrap_or(&Value::Null)),
        other => Err(format!("unknown command `{other}`")),
    }
}
