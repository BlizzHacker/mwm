//! Latest published MWM release on GitHub. Used by the About page's
//! "Check for updates" so desktop and server builds share one code path
//! (the WebView CSP blocks direct fetches to api.github.com).

use std::time::Duration;

use anyhow::{bail, Context};
use serde_json::{json, Value};

pub const REPO: &str = "BlizzHacker/mwm";

/// Latest release tag, its URL, and whether it is newer than this build.
pub fn latest() -> anyhow::Result<Value> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build();
    let resp = agent
        .get(&format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .set("User-Agent", "MWM-update-check")
        .set("Accept", "application/vnd.github+json")
        .call()
        .context("GitHub API unreachable")?;
    let body: Value = serde_json::from_reader(resp.into_reader())?;
    let tag = body["tag_name"].as_str().unwrap_or("").to_string();
    if !tag.starts_with('v') {
        bail!("unexpected release tag {tag:?}");
    }
    let newer = version_newer(&tag[1..], crate::VERSION);
    let mut assets = Vec::new();
    for a in body["assets"].as_array().cloned().unwrap_or_default() {
        if let (Some(name), Some(url)) = (a["name"].as_str(), a["browser_download_url"].as_str()) {
            assets.push(json!({"name": name, "url": url, "size": a["size"]}));
        }
    }
    Ok(json!({
        "tag": tag,
        "url": body["html_url"],
        "published": body["published_at"],
        "notes": body["body"].as_str().unwrap_or("").chars().take(4000).collect::<String>(),
        "current": crate::VERSION,
        "update_available": newer,
        "assets": assets,
    }))
}

/// Semver-ish compare: true when  >  (numeric parts, extra parts count).
pub fn version_newer(a: &str, b: &str) -> bool {
    let pa: Vec<u64> = a.split('.').map(|x| x.trim().parse().unwrap_or(0)).collect();
    let pb: Vec<u64> = b.split('.').map(|x| x.trim().parse().unwrap_or(0)).collect();
    let n = pa.len().max(pb.len());
    for i in 0..n {
        let (x, y) = (*pa.get(i).unwrap_or(&0), *pb.get(i).unwrap_or(&0));
        if x != y { return x > y; }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::version_newer;
    #[test]
    fn compare() {
        assert!(version_newer("5.2.0", "5.1.0"));
        assert!(version_newer("5.1.1", "5.1.0"));
        assert!(version_newer("6.0.0", "5.9.9"));
        assert!(!version_newer("5.1.0", "5.1.0"));
        assert!(!version_newer("5.0.9", "5.1.0"));
        assert!(version_newer("5.1.0.1", "5.1.0"));
    }
}
