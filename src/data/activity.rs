use std::{
    fs::{self},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use serde_json::Value;
use walkdir::WalkDir;

use crate::config::SourceAliases;
use crate::data::utils::{
    normalize_inline, normalize_sort_key, parse_date_key, pick_plain_string, read_records_tail,
    resolve_optional_subdir, truncate_text,
};

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
        let records = read_records_tail(&path, remaining.min(80))?;
        for value in records.into_iter().rev() {
            events.push(parse_activity_event_preview(&value, &path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown.json")));
        }
    }

    events.truncate(max_events);
    Ok(events)
}

fn parse_activity_event_preview(value: &Value, source_file: &str) -> ActivityEventPreview {
    let timestamp = pick_plain_string(
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

    let event_type = pick_plain_string(value, &["event_type", "type", "name", "action"])
        .unwrap_or_else(|| "unknown".to_owned());

    let summary = build_activity_summary(value);
    let sort_key = normalize_sort_key(&timestamp);
    let date_key = parse_date_key(&timestamp);
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
