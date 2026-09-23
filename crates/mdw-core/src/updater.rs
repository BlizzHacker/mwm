//! Software Updater. Uses the OS package manager (winget / apt / brew) -
//! never a private download mirror - so updates come from the same place the
//! publisher ships them.

use serde::Serialize;

use crate::util;

#[derive(Debug, Clone, Serialize)]
pub struct Update {
    pub id: String,
    pub name: String,
    pub current: String,
    pub available: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub id: String,
    pub ok: bool,
    pub message: String,
}

/// Parse winget's fixed-width table. Column starts come from the header row,
/// counted in characters because names can contain multi-byte text.
pub fn parse_winget_table(text: &str) -> Vec<Update> {
    let lines: Vec<String> = text
        .lines()
        .map(|l| l.rsplit('\r').next().unwrap_or("").to_string())
        .collect();
    let Some(h) = lines.iter().position(|l| {
        let t = l.trim_start();
        t.starts_with("Name") && l.contains(" Id ") && l.contains("Version") && l.contains("Available")
    }) else {
        return vec![];
    };
    let header: Vec<char> = lines[h].chars().collect();
    let find = |word: &str| -> Option<usize> {
        let w: Vec<char> = word.chars().collect();
        (0..header.len().saturating_sub(w.len() - 1)).find(|&i| header[i..i + w.len()] == w[..] && (i == 0 || header[i - 1] == ' '))
    };
    let (Some(p_id), Some(p_ver), Some(p_av)) = (find("Id"), find("Version"), find("Available")) else {
        return vec![];
    };
    let p_src = find("Source");
    let name_start = header.iter().position(|c| *c != ' ').unwrap_or(0);
    let mut out = Vec::new();
    for line in lines.iter().skip(h + 1) {
        let t = line.trim();
        if t.chars().all(|c| c == '-' || c == '─') && !t.is_empty() {
            continue;
        }
        if t.is_empty() || t.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) && t.contains("upgrade") {
            break;
        }
        let c: Vec<char> = line.chars().collect();
        if c.len() <= p_av {
            continue;
        }
        let col = |a: usize, b: Option<usize>| -> String {
            let b = b.unwrap_or(c.len()).min(c.len());
            if a >= b {
                return String::new();
            }
            c[a..b].iter().collect::<String>().trim().to_string()
        };
        let id = col(p_id, Some(p_ver));
        if id.is_empty() || id.contains(' ') {
            continue;
        }
        out.push(Update {
            name: col(name_start, Some(p_id)),
            id,
            current: col(p_ver, Some(p_av)),
            available: col(p_av, p_src),
            source: p_src.map(|s| col(s, None)).unwrap_or_else(|| "winget".into()),
        });
    }
    out
}

#[cfg(windows)]
pub fn list() -> anyhow::Result<Vec<Update>> {
    let (_, out) = util::run_capture(
        "winget",
        &["upgrade", "--accept-source-agreements", "--disable-interactivity"],
    )?;
    Ok(parse_winget_table(&out))
}

#[cfg(windows)]
pub fn apply(id: &str) -> Outcome {
    let res = util::run_capture(
        "winget",
        &[
            "upgrade", "--id", id, "--exact", "--silent", "--accept-package-agreements",
            "--accept-source-agreements", "--disable-interactivity",
        ],
    );
    finish(id, res)
}

#[cfg(target_os = "linux")]
pub fn list() -> anyhow::Result<Vec<Update>> {
    let (_, out) = util::run_capture("apt", &["list", "--upgradable"])?;
    Ok(out
        .lines()
        .filter(|l| l.contains("[upgradable from:"))
        .filter_map(|l| {
            // pkg/suite 1.2.3 amd64 [upgradable from: 1.2.2]
            let (pkg, rest) = l.split_once('/')?;
            let f: Vec<&str> = rest.split_whitespace().collect();
            let current = l.split("from:").nth(1)?.trim().trim_end_matches(']').to_string();
            Some(Update { id: pkg.into(), name: pkg.into(), current, available: f.get(1)?.to_string(), source: "apt".into() })
        })
        .collect())
}

#[cfg(target_os = "linux")]
pub fn apply(id: &str) -> Outcome {
    finish(id, util::run_capture("apt-get", &["install", "--only-upgrade", "-y", id]))
}

#[cfg(target_os = "macos")]
pub fn list() -> anyhow::Result<Vec<Update>> {
    let (_, out) = util::run_capture("brew", &["outdated", "--verbose"])?;
    Ok(out
        .lines()
        .filter_map(|l| {
            // name (1.0) < 1.1
            let (name, rest) = l.split_once(' ')?;
            let (cur, avail) = rest.split_once('<')?;
            Some(Update {
                id: name.into(),
                name: name.into(),
                current: cur.trim().trim_matches(['(', ')']).into(),
                available: avail.trim().into(),
                source: "brew".into(),
            })
        })
        .collect())
}

#[cfg(target_os = "macos")]
pub fn apply(id: &str) -> Outcome {
    finish(id, util::run_capture("brew", &["upgrade", id]))
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn list() -> anyhow::Result<Vec<Update>> {
    Ok(vec![])
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn apply(id: &str) -> Outcome {
    Outcome { id: id.into(), ok: false, message: "unsupported platform".into() }
}

#[allow(dead_code)]
fn finish(id: &str, res: anyhow::Result<(i32, String)>) -> Outcome {
    match res {
        Ok((code, out)) => {
            let tail: Vec<&str> = out.lines().map(|l| l.rsplit('\r').next().unwrap_or("").trim()).filter(|l| !l.is_empty()).collect();
            let msg = tail.iter().rev().take(3).rev().cloned().collect::<Vec<_>>().join(" / ");
            Outcome { id: id.into(), ok: code == 0, message: if msg.is_empty() { format!("exit code {code}") } else { msg } }
        }
        Err(e) => Outcome { id: id.into(), ok: false, message: e.to_string() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_winget() {
        let sample = "   - \r   \\ \r\
Name                               Id                          Version      Available    Source\n\
------------------------------------------------------------------------------------------------\n\
7-Zip 25.01 (x64)                  7zip.7zip                   25.01        26.03        winget\n\
Mozilla Thunderbird (x64 en-US)    Mozilla.Thunderbird         154.0        156.0        winget\n\
Microsoft Visual Studio Code (Us…  Microsoft.VisualStudioCode  1.124.2      1.138.0      winget\n\
3 upgrades available.\n";
        let u = parse_winget_table(sample);
        assert_eq!(u.len(), 3);
        assert_eq!(u[0].id, "7zip.7zip");
        assert_eq!(u[0].current, "25.01");
        assert_eq!(u[0].available, "26.03");
        assert_eq!(u[2].id, "Microsoft.VisualStudioCode");
        assert_eq!(u[2].name, "Microsoft Visual Studio Code (Us…");
        assert_eq!(u[1].source, "winget");
    }
}
