use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use super::hash::hex_lower;

#[derive(Debug)]
pub(crate) struct DownloadTask {
    pub url: String,
    pub output_path: PathBuf,
    pub temp_path: PathBuf,
}

pub(crate) fn download_to_temp_and_hash(url: &str, temp_path: &Path) -> Result<String> {
    let response = ureq::get(url)
        .header("User-Agent", "Mozilla/5.0")
        .call()
        .with_context(|| format!("request failed for {url}"))?;

    let mut reader = response.into_body().into_reader();
    let mut file = File::create(temp_path)
        .with_context(|| format!("failed to create {}", temp_path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];

    loop {
        let n = reader.read(&mut buffer).with_context(|| format!("failed while reading response body for {url}"))?;
        if n == 0 { break; }
        file.write_all(&buffer[..n]).with_context(|| format!("failed while writing {}", temp_path.display()))?;
        hasher.update(&buffer[..n]);
    }

    file.flush().with_context(|| format!("failed to flush {}", temp_path.display()))?;
    Ok(hex_lower(&hasher.finalize()))
}

pub(crate) fn finalize_temp_file(temp_path: &Path, output_path: &Path) -> Result<()> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    }

    match fs::rename(temp_path, output_path) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(temp_path, output_path).with_context(|| format!("failed to copy {} to {}", temp_path.display(), output_path.display()))?;
            let _ = fs::remove_file(temp_path);
            Ok(())
        }
    }
}
