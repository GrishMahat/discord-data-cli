use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde_json::Value;

pub(crate) fn read_json_value(path: &Path) -> Result<Value> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).with_context(|| format!("invalid JSON in {}", path.display()))
}

pub(crate) fn read_records_json_or_ndjson(path: &Path) -> Result<Vec<Value>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    
    // Attempt standard JSON first
    match serde_json::from_reader::<_, Value>(reader) {
        Ok(Value::Array(items)) => Ok(items),
        Ok(Value::Object(mut map)) => {
            if let Some(Value::Array(items)) = map.remove("messages") {
                Ok(items)
            } else {
                Ok(vec![Value::Object(map)])
            }
        }
        Ok(single) => Ok(vec![single]),
        Err(_) => {
            // JSON failed, try parsing line by line (NDJSON)
            let file = File::open(path)
                .with_context(|| format!("failed to re-open {}", path.display()))?;
            let reader = BufReader::new(file);
            let mut rows = Vec::new();
            for line in reader.lines() {
                let line = line?;
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(value) = serde_json::from_str::<Value>(line) {
                    rows.push(value);
                }
            }
            Ok(rows)
        }
    }
}

pub(crate) fn count_records(path: &Path) -> Result<usize> {
    // For efficiency, we can try counting without storing everything if it's large,
    // but for now read_records_json_or_ndjson is simpler.
    Ok(read_records_json_or_ndjson(path)?.len())
}

pub(crate) fn find_file_case_insensitive(dir: &Path, file_name: &str) -> Result<Option<PathBuf>> {
    let target = file_name.to_ascii_lowercase();
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        if !entry.path().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name == target {
            return Ok(Some(entry.path()));
        }
    }
    Ok(None)
}

pub(crate) fn resolve_optional_subdir(package_dir: &Path, names: &[String]) -> Result<Option<PathBuf>> {
    let mut normalized_dirs = BTreeMap::new();
    for entry in fs::read_dir(package_dir)
        .with_context(|| format!("failed to read {}", package_dir.display()))?
    {
        let entry = entry?;
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        normalized_dirs.insert(normalize_dir_name(&name), entry.path());
    }

    for name in names {
        let key = normalize_dir_name(name);
        if let Some(path) = normalized_dirs.get(&key) {
            return Ok(Some(path.clone()));
        }
    }
    Ok(None)
}

pub(crate) fn normalize_dir_name(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

pub(crate) fn value_to_plain_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null => None,
        _ => Some(value.to_string()),
    }
}

pub(crate) fn pick_str<'a>(record: &'a Value, keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        if let Some(Value::String(text)) = record.get(*key) {
            return Some(text);
        }
    }
    None
}

pub(crate) fn pick_plain_string(record: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = record.get(*key)
            && let Some(text) = value_to_plain_string(value)
        {
            let normalized = text.trim();
            if !normalized.is_empty() {
                return Some(normalized.to_owned());
            }
        }
    }
    None
}

pub(crate) fn pick_timestamp_month(record: &Value, keys: &[&str]) -> Option<String> {
    pick_plain_string(record, keys).and_then(|text| parse_year_month(&text))
}

pub(crate) fn parse_year_month(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    if bytes.len() < 7 {
        return None;
    }

    let valid_sep = matches!(bytes[4], b'-' | b'/' | b'.');
    if !valid_sep
        || !bytes[0..4].iter().all(u8::is_ascii_digit)
        || !bytes[5..7].iter().all(u8::is_ascii_digit)
    {
        return None;
    }

    let year = std::str::from_utf8(&bytes[0..4]).ok()?;
    let month = std::str::from_utf8(&bytes[5..7]).ok()?;
    if !(("01"..="12").contains(&month)) {
        return None;
    }

    Some(format!("{year}-{month}"))
}

pub(crate) fn extract_message_content(record: &Value) -> String {
    for key in ["Contents", "Content", "content", "message_content"] {
        if let Some(value) = record.get(key)
            && let Some(s) = value_to_plain_string(value)
        {
            return s;
        }
    }
    String::new()
}

pub(crate) fn extract_attachment_urls(record: &Value) -> Vec<String> {
    for key in ["Attachments", "attachments"] {
        if let Some(value) = record.get(key) {
            return attachment_value_to_urls(value);
        }
    }
    Vec::new()
}

pub(crate) fn attachment_value_to_urls(value: &Value) -> Vec<String> {
    match value {
        Value::String(text) => text
            .split_whitespace()
            .map(ToOwned::to_owned)
            .filter(|s| !s.is_empty())
            .collect(),
        Value::Array(items) => {
            let mut out = Vec::new();
            for item in items {
                match item {
                    Value::String(s) => out.push(s.clone()),
                    Value::Object(map) => {
                        if let Some(url) = map.get("url").and_then(value_to_plain_string) {
                            out.push(url);
                        }
                    }
                    _ => {}
                }
            }
            out
        }
        Value::Object(map) => map
            .get("url")
            .and_then(value_to_plain_string)
            .map(|s| vec![s])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

pub(crate) fn channel_title(channel: Option<&Value>, fallback_id: &str) -> String {
    let Some(channel) = channel else {
        return fallback_id.to_owned();
    };

    if let Some(name) = channel
        .get("name")
        .and_then(value_to_plain_string)
        .filter(|s| !s.trim().is_empty())
    {
        return name;
    }

    if let Some(Value::Array(recipients)) = channel.get("recipients") {
        let names: Vec<String> = recipients
            .iter()
            .filter_map(|item| {
                if let Value::Object(map) = item {
                    for key in ["global_name", "username", "name", "id"] {
                        if let Some(value) = map.get(key).and_then(value_to_plain_string) {
                            return Some(value);
                        }
                    }
                }
                value_to_plain_string(item)
            })
            .take(4)
            .collect();

        if !names.is_empty() {
            return names.join(", ");
        }
    }

    fallback_id.to_owned()
}
