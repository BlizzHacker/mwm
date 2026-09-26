//! Opaque KDBX storage. Decryption and the master password stay in the UI;
//! the MWM agent stores only the original KeePass-compatible database bytes.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use anyhow::{bail, Context};
use base64::Engine;
use serde::Serialize;
use serde_json::Value;

const MAX_KDBX_BYTES: usize = 10 * 1024 * 1024;
const MAX_BLOB_BYTES: usize = 14 * 1024 * 1024;

#[derive(Serialize)]
pub struct StoredVault {
    pub revision: String,
    pub blob: Option<Value>,
}

fn path() -> std::path::PathBuf {
    crate::util::data_dir().join("vault.json")
}

fn revision(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn read_at(path: &Path) -> anyhow::Result<StoredVault> {
    if !path.exists() {
        return Ok(StoredVault { revision: "missing".into(), blob: None });
    }
    let bytes = fs::read(path).context("read encrypted vault")?;
    if bytes.len() > MAX_BLOB_BYTES {
        bail!("encrypted vault is too large");
    }
    let blob: Value = serde_json::from_slice(&bytes).context("encrypted vault file is invalid")?;
    validate(&blob)?;
    Ok(StoredVault { revision: revision(&bytes), blob: Some(blob) })
}

fn validate(blob: &Value) -> anyhow::Result<()> {
    if blob.get("format").and_then(Value::as_str) != Some("kdbx") { bail!("unsupported vault format"); }
    let data = blob.get("data").and_then(Value::as_str).ok_or_else(|| anyhow::anyhow!("missing KDBX data"))?;
    let raw = base64::engine::general_purpose::STANDARD.decode(data).context("invalid KDBX encoding")?;
    if raw.len() > MAX_KDBX_BYTES || !raw.starts_with(&[0x03, 0xD9, 0xA2, 0x9A, 0x67, 0xFB, 0x4B, 0xB5]) {
        bail!("invalid or oversized KDBX database");
    }
    Ok(())
}

fn write_at(path: &Path, expected: &str, blob: &Value) -> anyhow::Result<StoredVault> {
    validate(blob)?;
    let bytes = serde_json::to_vec(blob)?;
    if bytes.len() > MAX_BLOB_BYTES {
        bail!("encrypted vault is too large");
    }
    let current = read_at(path)?;
    if current.revision != expected {
        bail!("vault changed on this machine; reload it before saving");
    }
    let tmp = path.with_extension(format!("{}.tmp", crate::web::random_hex(8)));
    let result = (|| -> anyhow::Result<()> {
        let mut opts = OpenOptions::new();
        opts.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file = opts.open(&tmp).context("create encrypted vault temp file")?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, path).context("replace encrypted vault")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result?;
    Ok(StoredVault { revision: revision(&bytes), blob: Some(blob.clone()) })
}

pub fn load() -> anyhow::Result<StoredVault> {
    read_at(&path())
}

pub fn store(expected_revision: &str, blob: &Value) -> anyhow::Result<StoredVault> {
    write_at(&path(), expected_revision, blob)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn opaque_vault_requires_matching_revision() {
        let dir = std::env::temp_dir().join(format!("mwm-vault-test-{}", crate::web::random_hex(8)));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("vault.json");
        let blob = json!({"format":"kdbx","data":"A9mimmf7S7U="});
        assert_eq!(read_at(&path).unwrap().revision, "missing");
        let saved = write_at(&path, "missing", &blob).unwrap();
        assert!(write_at(&path, "missing", &blob).is_err());
        assert_eq!(read_at(&path).unwrap().revision, saved.revision);
        assert!(write_at(&path, &saved.revision, &blob).is_ok());
        fs::remove_dir_all(dir).unwrap();
    }
}
