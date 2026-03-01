use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde_json::Value;
use walkdir::WalkDir;

use crate::config::SourceAliases;

#[derive(Debug, Clone)]
pub struct SupportTicketView {
    pub id: String,
    pub subject: String,
    pub status: String,
    pub priority: String,
    pub created_at: String,
    pub updated_at: String,
    pub comment_count: usize,
    pub detail_lines: Vec<String>,
}

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

pub fn load_support_tickets(
    package_dir: &Path,
    aliases: &SourceAliases,
) -> Result<Vec<SupportTicketView>> {
    let Some(support_dir) = resolve_optional_subdir(package_dir, &aliases.support_tickets)? else {
        return Ok(Vec::new());
    };
    let Some(tickets_path) = find_file_case_insensitive(&support_dir, "tickets.json")? else {
        return Ok(Vec::new());
    };

    let tickets_value = read_json_value(&tickets_path)?;
    let mut tickets = Vec::new();
    match tickets_value {
        Value::Array(items) => {
            for value in items {
                if let Some(ticket) = support_ticket_from_value(&value) {
                    tickets.push(ticket);
                }
            }
        }
        Value::Object(map) => {
            for (_, value) in map {
                if let Some(ticket) = support_ticket_from_value(&value) {
                    tickets.push(ticket);
                }
            }
        }
        _ => {}
    }

    tickets.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| b.created_at.cmp(&a.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });

    Ok(tickets)
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

fn support_ticket_from_value(value: &Value) -> Option<SupportTicketView> {
    if !value.is_object() {
        return None;
    }

    let id = pick_value_string(value, &["id", "ticket_id", "case_id", "number"])
        .unwrap_or_else(|| "unknown".to_owned());
    let subject = pick_value_string(value, &["subject", "title", "reason", "topic"])
        .unwrap_or_else(|| "<no subject>".to_owned());
    let status = pick_value_string(value, &["status", "ticket_status", "state"])
        .unwrap_or_else(|| "unknown".to_owned());
    let priority = pick_value_string(value, &["priority", "severity", "urgency"])
        .unwrap_or_else(|| "unknown".to_owned());
    let created_at = pick_value_string(
        value,
        &["created_at", "createdAt", "opened_at", "openedAt", "date"],
    )
    .unwrap_or_else(|| "unknown".to_owned());
    let updated_at = pick_value_string(
        value,
        &[
            "updated_at",
            "updatedAt",
            "last_activity_at",
            "lastActivityAt",
            "closed_at",
            "closedAt",
        ],
    )
    .unwrap_or_else(|| created_at.clone());
    let comment_count = value
        .get("comments")
        .and_then(|v| match v {
            Value::Array(items) => Some(items.len()),
            _ => None,
        })
        .unwrap_or(0);

    Some(SupportTicketView {
        id,
        subject,
        status,
        priority,
        created_at,
        updated_at,
        comment_count,
        detail_lines: build_ticket_detail_lines(value),
    })
}

fn build_ticket_detail_lines(ticket: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "Status: {}",
        pick_value_string(ticket, &["status", "ticket_status", "state"])
            .unwrap_or_else(|| "unknown".to_owned())
    ));
    lines.push(format!(
        "Priority: {}",
        pick_value_string(ticket, &["priority", "severity", "urgency"])
            .unwrap_or_else(|| "unknown".to_owned())
    ));
    lines.push(format!(
        "Created: {}",
        pick_value_string(
            ticket,
            &["created_at", "createdAt", "opened_at", "openedAt", "date"]
        )
        .unwrap_or_else(|| "unknown".to_owned())
    ));
    lines.push(format!(
        "Updated: {}",
        pick_value_string(
            ticket,
            &[
                "updated_at",
                "updatedAt",
                "last_activity_at",
                "lastActivityAt",
                "closed_at",
                "closedAt",
            ]
        )
        .unwrap_or_else(|| "unknown".to_owned())
    ));

    let mut overview_fields = Vec::new();
    if let Value::Object(map) = ticket {
        for key in [
            "subject",
            "title",
            "category",
            "type",
            "assignee",
            "assigned_to",
        ] {
            if let Some(value) = map.get(key).and_then(value_to_plain_string) {
                let value = value.trim();
                if !value.is_empty() {
                    overview_fields.push(format!("{key}: {value}"));
                }
            }
        }
    }
    if !overview_fields.is_empty() {
        lines.push(String::new());
        lines.push("Fields:".to_owned());
        lines.extend(overview_fields.into_iter().map(|f| format!("  {f}")));
    }

    if let Some(Value::Array(comments)) = ticket.get("comments") {
        lines.push(String::new());
        lines.push(format!("Comments ({})", comments.len()));
        for (idx, comment) in comments.iter().enumerate() {
            let ts = pick_value_string(comment, &["created_at", "createdAt", "date", "timestamp"])
                .unwrap_or_else(|| "unknown-time".to_owned());
            let author = pick_value_string(
                comment,
                &[
                    "author",
                    "author_name",
                    "staff_name",
                    "agent",
                    "username",
                    "user",
                ],
            )
            .unwrap_or_else(|| "unknown-author".to_owned());
            let content =
                pick_value_string(comment, &["comment", "content", "body", "message", "text"])
                    .or_else(|| Some(comment.to_string()))
                    .unwrap_or_else(|| "<empty>".to_owned());
            lines.push(format!("  {}. [{}] {}", idx + 1, ts, author));
            push_multiline_prefixed(&mut lines, "     ", &content);

            if let Some(user_agent) =
                pick_value_string(comment, &["user_agent", "userAgent", "client", "browser"])
            {
                lines.push(format!("     user_agent: {user_agent}"));
            }
            if let Some(device) = pick_value_string(comment, &["device", "platform", "os"]) {
                lines.push(format!("     device: {device}"));
            }
            if let Some(ip) = pick_value_string(comment, &["ip", "ip_address", "remote_ip"]) {
                lines.push(format!("     ip: {ip}"));
            }
            if let Some(locale) = pick_value_string(comment, &["locale", "language"]) {
                lines.push(format!("     locale: {locale}"));
            }
            if let Some(status) = pick_value_string(comment, &["status", "state"]) {
                lines.push(format!("     status: {status}"));
            }

            if let Ok(raw_comment) = serde_json::to_string_pretty(comment) {
                lines.push("     raw_comment:".to_owned());
                for line in raw_comment.lines() {
                    lines.push(format!("       {line}"));
                }
            }
        }
    }

    lines
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

fn find_file_case_insensitive(dir: &Path, file_name: &str) -> Result<Option<PathBuf>> {
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

fn read_json_value(path: &Path) -> Result<Value> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).with_context(|| format!("invalid JSON in {}", path.display()))
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

fn push_multiline_prefixed(lines: &mut Vec<String>, prefix: &str, text: &str) {
    if text.is_empty() {
        lines.push(format!("{prefix}<empty>"));
        return;
    }
    for line in text.lines() {
        lines.push(format!("{prefix}{line}"));
    }
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
