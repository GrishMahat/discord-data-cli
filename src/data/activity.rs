use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde_json::Value;
use walkdir::WalkDir;

use crate::config::SourceAliases;

#[derive(Debug, Clone)]
pub struct ActivityEventPreview {
    pub timestamp: String,
    pub event_type: String,
    pub summary: String,
    pub source_file: String,
    pub date_key: Option<String>,
    pub sort_key: String,
    pub detail: String,
}

pub fn load_recent_activity_events(
    package_dir: &Path,
    aliases: &SourceAliases,
    max_events: usize,
) -> Result<Vec<ActivityEventPreview>> {
    let Some(activity_dir) = resolve_optional_subdir(package_dir, &aliases.activity)? else {
        return Ok(Vec::new());
    };

    let mut files: Vec<(PathBuf, SystemTime)> = WalkDir::new(&activity_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| {
            path.extension()
                .map(|ext| ext.to_string_lossy().eq_ignore_ascii_case("json"))
                .unwrap_or(false)
        })
        .map(|path| {
            let modified = fs::metadata(&path)
                .and_then(|m| m.modified())
                .unwrap_or(UNIX_EPOCH);
            (path, modified)
        })
        .collect();

    files.sort_by(|a, b| b.1.cmp(&a.1));

    let mut events = Vec::new();
    for (path, _) in files {
        if events.len() >= max_events {
            break;
        }
        let remaining = max_events - events.len();
        let mut from_file =
            read_recent_events_from_file(&path, remaining.min(80), 2 * 1024 * 1024)?;
        events.append(&mut from_file);
    }

    events.truncate(max_events);
    Ok(events)
}

fn read_recent_events_from_file(
    path: &Path,
    max_events: usize,
    max_tail_bytes: u64,
) -> Result<Vec<ActivityEventPreview>> {
    if max_events == 0 {
        return Ok(Vec::new());
    }

    let mut file =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let file_size = file.metadata().map(|m| m.len()).unwrap_or(0);
    let start = file_size.saturating_sub(max_tail_bytes);
    file.seek(SeekFrom::Start(start))
        .with_context(|| format!("failed to seek {}", path.display()))?;

    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .with_context(|| format!("failed to read {}", path.display()))?;

    if start > 0
        && let Some(pos) = buffer.iter().position(|&b| b == b'\n')
    {
        buffer.drain(0..=pos);
    }

    let source_file = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown.json")
        .to_owned();

    let mut events = Vec::new();
    for line in String::from_utf8_lossy(&buffer).lines().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            events.push(parse_activity_event_preview(&value, &source_file));
            if events.len() >= max_events {
                break;
            }
        }
    }

    Ok(events)
}

fn parse_activity_event_preview(value: &Value, source_file: &str) -> ActivityEventPreview {
    let timestamp = pick_value_string(
        value,
        &[
            "timestamp",
            "date",
            "created_at",
            "createdAt",
            "updated_at",
            "updatedAt",
            "occurred_at",
            "time",
        ],
    )
    .unwrap_or_else(|| "?".to_owned());

    let event_type = pick_value_string(value, &["event_type", "type", "name", "action"])
        .unwrap_or_else(|| "unknown".to_owned());

    let summary = build_activity_summary(value);
    let sort_key = normalize_sort_key(&timestamp);
    let date_key = extract_date_key(&timestamp);
    let detail = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());

    ActivityEventPreview {
        timestamp,
        event_type,
        summary,
        source_file: source_file.to_owned(),
        date_key,
        sort_key,
        detail,
    }
}

fn build_activity_summary(value: &Value) -> String {
    let headline = pick_scalar_string(
        value,
        &[
            "description",
            "summary",
            "details",
            "message",
            "title",
            "reason",
            "activity",
        ],
    )
    .map(|text| normalize_inline(&text));
    if let Some(headline) = headline.filter(|text| !text.is_empty()) {
        return truncate_text(&headline, 240);
    }

    let actor = pick_scalar_string(
        value,
        &[
            "actor",
            "actor_name",
            "author",
            "username",
            "user",
            "member",
            "initiator",
        ],
    )
    .map(|v| truncate_text(&normalize_inline(&v), 72));
    let target = pick_scalar_string(
        value,
        &[
            "target",
            "target_name",
            "channel",
            "channel_name",
            "guild",
            "guild_name",
            "ticket_id",
            "message_id",
        ],
    )
    .map(|v| truncate_text(&normalize_inline(&v), 96));
    let action = pick_scalar_string(value, &["action", "event", "name", "status", "result"])
        .map(|v| truncate_text(&normalize_inline(&v), 72));

    let mut parts = Vec::new();
    if let Some(actor) = actor {
        parts.push(format!("by {actor}"));
    }
    if let Some(action) = action {
        parts.push(action);
    }
    if let Some(target) = target {
        parts.push(format!("on {target}"));
    }
    if !parts.is_empty() {
        return parts.join(" • ");
    }

    if let Value::Object(map) = value {
        let mut kv = Vec::new();
        for (key, val) in map {
            if let Some(scalar) = value_to_scalar_string(val) {
                let inline = normalize_inline(&scalar);
                if inline.is_empty() {
                    continue;
                }
                kv.push(format!("{key}={}", truncate_text(&inline, 48)));
                if kv.len() >= 4 {
                    break;
                }
            }
        }
        if !kv.is_empty() {
            return kv.join(" • ");
        }
    }

    truncate_text(&normalize_inline(&value.to_string()), 240)
}

fn resolve_optional_subdir(package_dir: &Path, names: &[String]) -> Result<Option<PathBuf>> {
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

fn normalize_dir_name(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

fn value_to_plain_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null => None,
        _ => Some(value.to_string()),
    }
}

fn pick_value_string(record: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = record.get(*key)
            && let Some(text) = value_to_plain_string(value)
        {
            let text = text.trim();
            if !text.is_empty() {
                return Some(text.to_owned());
            }
        }
    }
    None
}

fn pick_scalar_string(record: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = record.get(*key)
            && let Some(text) = value_to_scalar_string(value)
        {
            let text = text.trim();
            if !text.is_empty() {
                return Some(text.to_owned());
            }
        }
    }
    None
}

fn value_to_scalar_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null => None,
        Value::Array(_) | Value::Object(_) => None,
    }
}

fn normalize_inline(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let kept = max_chars.saturating_sub(1);
    format!("{}…", text.chars().take(kept).collect::<String>())
}

fn extract_date_key(timestamp: &str) -> Option<String> {
    let bytes = timestamp.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    let date = &bytes[0..10];
    if date[0..4].iter().all(u8::is_ascii_digit)
        && date[4] == b'-'
        && date[5..7].iter().all(u8::is_ascii_digit)
        && date[7] == b'-'
        && date[8..10].iter().all(u8::is_ascii_digit)
    {
        return Some(String::from_utf8_lossy(date).to_string());
    }
    None
}

fn normalize_sort_key(timestamp: &str) -> String {
    let normalized: String = timestamp.chars().filter(|ch| ch.is_ascii_digit()).collect();
    if normalized.is_empty() {
        timestamp.to_owned()
    } else {
        normalized
    }
}
