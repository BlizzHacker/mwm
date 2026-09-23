// MDW - Move Digital Weight desktop app: a thin Tauri shell over mdw-core.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use mdw_core::{apps, cleaner, disk, drivers, dupes, files, jobs, procs, shred, startup, sys, toolkit, updater};
use serde::Serialize;

type Res<T> = Result<T, String>;

/// Run blocking engine work off the UI thread.
async fn bg<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Res<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn system_info() -> Res<sys::SystemInfo> {
    bg(sys::info).await
}

#[tauri::command]
async fn cleaner_scan() -> Res<Vec<cleaner::ScanItem>> {
    bg(cleaner::scan_all).await
}

#[tauri::command]
async fn cleaner_clean(ids: Vec<String>) -> Res<Vec<cleaner::CleanResult>> {
    bg(move || cleaner::clean(&ids)).await
}

#[tauri::command]
async fn apps_list(store: bool) -> Res<Vec<apps::App>> {
    bg(move || apps::list(store)).await
}

#[tauri::command]
async fn app_uninstall(app: apps::App, quiet: bool) -> Res<apps::Outcome> {
    bg(move || apps::uninstall(&app, quiet)).await
}

#[tauri::command]
async fn app_leftovers(app: apps::App) -> Res<Vec<apps::Leftover>> {
    bg(move || apps::leftovers(&app)).await
}

#[tauri::command]
async fn leftovers_remove(items: Vec<apps::Leftover>) -> Res<apps::RemoveReport> {
    bg(move || apps::remove_leftovers(&items)).await
}

#[tauri::command]
async fn startup_list() -> Res<Vec<startup::StartupItem>> {
    bg(startup::list).await
}

#[tauri::command]
async fn startup_set(id: String, enabled: bool) -> Res<startup::Outcome> {
    bg(move || startup::set_enabled(&id, enabled)).await
}

#[tauri::command]
async fn service_mode(name: String, mode: String) -> Res<startup::Outcome> {
    bg(move || startup::set_service_mode(&name, &mode)).await
}

#[tauri::command]
async fn updates_list() -> Res<Vec<updater::Update>> {
    bg(updater::list).await?.map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_apply(id: String) -> Res<updater::Outcome> {
    bg(move || updater::apply(&id)).await
}

#[tauri::command]
async fn dupes_find(roots: Vec<String>, min_size: u64) -> Res<Vec<dupes::DupGroup>> {
    bg(move || dupes::find(&roots.iter().map(PathBuf::from).collect::<Vec<_>>(), min_size, false)).await
}

#[derive(Serialize)]
struct Removed {
    removed: Vec<String>,
    failed: Vec<(String, String)>,
}

#[tauri::command]
async fn dupes_remove(paths: Vec<String>) -> Res<Removed> {
    bg(move || {
        let (removed, failed) = dupes::remove(&paths);
        Removed { removed, failed }
    })
    .await
}

#[tauri::command]
async fn disk_analyze(root: String) -> Res<disk::Report> {
    bg(move || disk::analyze(&PathBuf::from(root), 60)).await
}

#[tauri::command]
async fn shred_paths(paths: Vec<String>, passes: u32) -> Res<shred::Report> {
    bg(move || shred::shred(&paths, passes)).await
}

#[tauri::command]
async fn wipe_free(dir: String) -> Res<String> {
    bg(move || shred::wipe_free_space(&dir)).await?.map_err(|e| e.to_string())
}

#[tauri::command]
async fn drivers_list() -> Res<Vec<drivers::Driver>> {
    bg(drivers::list).await
}

#[tauri::command]
async fn procs_list() -> Res<Vec<procs::Proc>> {
    bg(procs::list).await
}

#[tauri::command]
async fn procs_kill(pids: Vec<u32>) -> Res<usize> {
    bg(move || procs::kill(&pids)).await
}

#[tauri::command]
fn reveal(path: String) -> Res<()> {
    sys::reveal(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn launch_tool(tool: String) -> Res<()> {
    sys::launch_tool(&tool).map_err(|e| e.to_string())
}

#[tauri::command]
fn relaunch_admin(app: tauri::AppHandle) -> Res<()> {
    sys::relaunch_elevated("").map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

// ------------------------------------------------------------ Commander ----

fn e<T>(r: anyhow::Result<T>) -> Res<T> {
    r.map_err(|x| x.to_string())
}

#[tauri::command]
async fn files_roots() -> Res<Vec<files::Root>> {
    bg(files::roots).await
}

#[tauri::command]
async fn files_list(dir: String) -> Res<files::Listing> {
    e(bg(move || files::list(&dir)).await?)
}

#[tauri::command]
fn files_mkdir(parent: String, name: String) -> Res<String> {
    e(files::mkdir(&parent, &name))
}

#[tauri::command]
fn files_rename(path: String, name: String) -> Res<String> {
    e(files::rename(&path, &name))
}

#[tauri::command]
fn files_conflicts(sources: Vec<String>, dest: String) -> Vec<String> {
    files::conflicts(&sources, &dest)
}

#[tauri::command]
fn files_transfer(sources: Vec<String>, dest: String, mode: String, is_move: bool) -> u64 {
    files::transfer(sources, dest, mode, is_move)
}

#[tauri::command]
fn files_delete(paths: Vec<String>, permanent: bool) -> u64 {
    files::delete(paths, permanent)
}

#[tauri::command]
fn files_pack(sources: Vec<String>, archive: String) -> u64 {
    files::pack(sources, archive)
}

#[tauri::command]
fn files_unpack(archive: String, dest: String) -> u64 {
    files::unpack(archive, dest)
}

#[tauri::command]
fn files_search(root: String, pattern: String, text: String) -> u64 {
    files::search(root, pattern, text)
}

#[tauri::command]
async fn files_preview(path: String) -> Res<files::Preview> {
    e(bg(move || files::preview(&path)).await?)
}

#[tauri::command]
async fn files_props(path: String) -> Res<files::Props> {
    e(bg(move || files::properties(&path)).await?)
}

#[tauri::command]
async fn files_dir_sizes(paths: Vec<String>) -> Res<Vec<(String, u64)>> {
    bg(move || files::dir_sizes(&paths)).await
}

#[tauri::command]
fn files_multi_rename(plan: Vec<(String, String)>) -> Res<usize> {
    e(files::multi_rename(&plan))
}

#[tauri::command]
fn files_write(path: String, text: String) -> Res<()> {
    e(files::write_text(&path, &text))
}

#[tauri::command]
fn open_default(path: String) -> Res<()> {
    e(sys::open_default(&path))
}

#[tauri::command]
fn edit_file(path: String) -> Res<()> {
    e(sys::edit(&path))
}

#[tauri::command]
fn terminal(dir: String) -> Res<()> {
    e(sys::terminal(&dir))
}

#[tauri::command]
fn jobs_list() -> Vec<jobs::JobInfo> {
    jobs::list()
}

#[tauri::command]
fn job_cancel(id: u64) {
    jobs::cancel(id)
}

#[tauri::command]
fn jobs_clear() {
    jobs::clear_finished()
}

// -------------------------------------------------------------- Toolkit ----

#[tauri::command]
fn toolkit_tasks() -> Vec<toolkit::Task> {
    toolkit::TASKS.to_vec()
}

#[tauri::command]
fn toolkit_run(id: String) -> Res<u64> {
    e(toolkit::run_task(&id))
}

#[tauri::command]
async fn toolkit_report() -> Res<serde_json::Value> {
    bg(toolkit::report).await
}

#[tauri::command]
async fn toolkit_security() -> Res<serde_json::Value> {
    bg(toolkit::security).await
}

#[tauri::command]
async fn toolkit_events() -> Res<serde_json::Value> {
    bg(toolkit::events).await
}

#[tauri::command]
async fn toolkit_network() -> Res<serde_json::Value> {
    bg(toolkit::network).await
}

#[tauri::command]
async fn toolkit_wifi() -> Res<serde_json::Value> {
    bg(toolkit::wifi).await
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            system_info,
            cleaner_scan,
            cleaner_clean,
            apps_list,
            app_uninstall,
            app_leftovers,
            leftovers_remove,
            startup_list,
            startup_set,
            service_mode,
            updates_list,
            update_apply,
            dupes_find,
            dupes_remove,
            disk_analyze,
            shred_paths,
            wipe_free,
            drivers_list,
            procs_list,
            procs_kill,
            reveal,
            launch_tool,
            relaunch_admin,
            files_roots,
            files_list,
            files_mkdir,
            files_rename,
            files_conflicts,
            files_transfer,
            files_delete,
            files_pack,
            files_unpack,
            files_search,
            files_preview,
            files_props,
            files_dir_sizes,
            files_multi_rename,
            files_write,
            open_default,
            edit_file,
            terminal,
            jobs_list,
            job_cancel,
            jobs_clear,
            toolkit_tasks,
            toolkit_run,
            toolkit_report,
            toolkit_security,
            toolkit_events,
            toolkit_network,
            toolkit_wifi
        ])
        .run(tauri::generate_context!())
        .expect("MDW failed to start");
}
