use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{BufReader, Read, Write},
    path::Path,
};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct AttachmentHashIndex {
    #[serde(default = "hash_index_version")]
    pub version: u8,
    #[serde(default)]
    pub hashes: HashMap<String, String>,
}

impl Default for AttachmentHashIndex {
    fn default() -> Self {
        Self {
            version: 1,
            hashes: HashMap::new(),
        }
    }
}

fn hash_index_version() -> u8 {
    1
}

pub(crate) fn load_hash_index(path: &Path) -> Result<AttachmentHashIndex> {
    if !path.is_file() {
        return Ok(AttachmentHashIndex::default());
    }
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    let parsed = serde_json::from_reader::<_, AttachmentHashIndex>(reader);
    match parsed {
        Ok(index) => Ok(index),
        Err(_) => Ok(AttachmentHashIndex::default()),
    }
}

pub(crate) fn save_hash_index(path: &Path, index: &AttachmentHashIndex) -> Result<()> {
    let tmp_path = path.with_extension("json.tmp");
    let mut file = File::create(&tmp_path)
        .with_context(|| format!("failed to create {}", tmp_path.display()))?;
    serde_json::to_writer_pretty(&mut file, index)
        .with_context(|| format!("failed to write {}", tmp_path.display()))?;
    file.write_all(b"\n")
        .with_context(|| format!("failed to finalize {}", tmp_path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush {}", tmp_path.display()))?;

    fs::rename(&tmp_path, path).with_context(|| {
        format!(
            "failed to move hash index from {} to {}",
            tmp_path.display(),
            path.display()
        )
    })?;
    Ok(())
}

pub(crate) fn canonical_attachment_key(url: &str) -> String {
    let without_fragment = url.split('#').next().unwrap_or(url);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    if let Some(rest) = without_query
        .strip_prefix("https://")
        .or_else(|| without_query.strip_prefix("http://"))
    {
        rest.to_owned()
    } else {
        without_query.to_owned()
    }
}

pub(crate) fn hash_file_sha256(path: &Path) -> Result<String> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];

    loop {
        let n = reader
            .read(&mut buffer)
            .with_context(|| format!("failed while reading {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(hex_lower(&hasher.finalize()))
}

pub(crate) fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}
