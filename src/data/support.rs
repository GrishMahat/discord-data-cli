use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde_json::Value;

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

fn push_multiline_prefixed(lines: &mut Vec<String>, prefix: &str, text: &str) {
    if text.is_empty() {
        lines.push(format!("{prefix}<empty>"));
        return;
    }
    for line in text.lines() {
        lines.push(format!("{prefix}{line}"));
    }
}
