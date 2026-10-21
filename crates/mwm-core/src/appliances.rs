//! Appliance adapters and application-only update jobs. Credentials stay in root-owned helper configuration.
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use crate::{jobs, util};
const NETWORK: &str = "/usr/local/libexec/mwm-network-devices";
const MEDIA: &str = "/usr/local/libexec/mwm-media-apps";
fn capture(script: &str, args: &[&str]) -> Result<Value> {
    if !std::path::Path::new(script).exists() { bail!("Fleet adapter is not installed on this machine; select the fleet controller"); }
    let mut command=vec![script]; command.extend_from_slice(args);
    let (code,out)=util::run_capture("python3",&command)?;
    if code!=0 { bail!("{}",out.trim()); }
    serde_json::from_str(out.trim()).context("Invalid fleet adapter response")
}
pub fn status() -> Result<Value> { capture(NETWORK,&["status"]) }
pub fn action(action: &str, service: &str) -> Result<Value> {
    if !matches!(action,"check_updates"|"restart_service") || !service.bytes().all(|c| c.is_ascii_alphanumeric() || c==b'-') { bail!("Invalid appliance action"); }
    let action=action.to_owned(); let service=service.to_owned();
    let id=jobs::start("appliance","OPNsense management","appliances",move |job| {
        let r=capture(NETWORK,&[&action,&service])?;job.line(r.to_string());Ok("Appliance action completed".into())
    });Ok(json!({"job":id}))
}
pub fn media_report() -> Result<Value> {
    let path="/var/lib/mwm/media-apps.json";
    if !std::path::Path::new(path).exists() { return Ok(json!({"generated_at":null,"applications":[],"note":"Run an application scan"})); }
    let data=std::fs::read(path)?; if data.len()>4*1024*1024 {bail!("Application report is too large");} Ok(serde_json::from_slice(&data)?)
}
pub fn media_job(action: &str, vmid: &str) -> Result<Value> {
    if !matches!(action,"scan"|"update") || (action=="update" && (vmid.is_empty() || vmid.len()>9 || !vmid.bytes().all(|c|c.is_ascii_digit()))) {bail!("Invalid application target");}
    let action=action.to_owned();let vmid=vmid.to_owned();
    let id=jobs::start("media_app","Media application maintenance","appliances",move |job| {
        let r=capture(MEDIA,&[&action,&vmid])?;job.line(r.to_string());Ok("Application maintenance completed; OS packages unchanged".into())
    });Ok(json!({"job":id}))
}
