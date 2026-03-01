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
pub(crate) struct MessageChannel {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) kind: ChannelKind,
    pub(crate) message_count: usize,
    pub(crate) messages_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChannelKind {
    Dm,
    GroupDm,
    PublicThread,
    Voice,
    Guild,
    Other,
}

impl ChannelKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            ChannelKind::Dm => "DM",
            ChannelKind::GroupDm => "GROUP_DM",
            ChannelKind::PublicThread => "PUBLIC_THREAD",
            ChannelKind::Voice => "VOICE",
            ChannelKind::Guild => "GUILD",
            ChannelKind::Other => "OTHER",
        }
    }
}

pub(crate) fn load_channels(
    package_dir: &Path,
    source_aliases: &SourceAliases,
) -> Result<Vec<MessageChannel>> {
    let Some(messages_dir) = resolve_optional_subdir(package_dir, &source_aliases.messages)? else {
        return Ok(Vec::new());
    };

    let mut channels = Vec::new();
    for entry in fs::read_dir(&messages_dir)
        .with_context(|| format!("failed to read {}", messages_dir.display()))?
    {
        let entry = entry?;
        let channel_dir = entry.path();
        if !channel_dir.is_dir() {
            continue;
        }

        let channel_json_path = channel_dir.join("channel.json");
        let messages_path = channel_dir.join("messages.json");
        if !messages_path.exists() {
            continue;
        }

        let channel_json = if channel_json_path.exists() {
            read_json_value(&channel_json_path).ok()
        } else {
            None
        };

        let id = channel_json
            .as_ref()
            .and_then(|v| v.get("id"))
            .and_then(value_to_plain_string)
            .unwrap_or_else(|| {
                channel_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_owned()
            });

        let title = channel_title(channel_json.as_ref(), &id);
        let kind = detect_channel_kind(channel_json.as_ref());
        let message_count = count_messages(&messages_path).unwrap_or(0);

        channels.push(MessageChannel {
            id,
            title,
            kind,
            message_count,
            messages_path,
        });
    }

    channels.sort_by(|a, b| {
        b.message_count
            .cmp(&a.message_count)
            .then_with(|| a.title.cmp(&b.title))
    });

    Ok(channels)
}

pub(crate) fn load_message_preview(
    channel: &MessageChannel,
    preview_count: usize,
) -> Result<Vec<String>> {
    let messages = read_messages(&channel.messages_path)?;
    if messages.is_empty() {
        return Ok(Vec::new());
    }

    let start = messages.len().saturating_sub(preview_count);
    let mut lines = Vec::new();

    for record in &messages[start..] {
        let timestamp = pick_str(record, &["Timestamp", "timestamp", "timestamp_ms", "date"])
            .unwrap_or("unknown");
        let content = pick_str(
            record,
            &["Contents", "Content", "content", "message_content"],
        )
        .unwrap_or("");
        let attachments = pick_str(record, &["Attachments", "attachments"]).unwrap_or("");

        let line = if content.is_empty() && attachments.is_empty() {
            format!("- [{timestamp}] <empty>")
        } else if attachments.is_empty() {
            format!("- [{timestamp}] {content}")
        } else {
            format!("- [{timestamp}] {content} | attachments: {attachments}")
        };
        lines.push(line);
    }

    Ok(lines)
}

fn read_json_value(path: &Path) -> Result<Value> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).with_context(|| format!("invalid JSON in {}", path.display()))
}

fn read_messages(path: &Path) -> Result<Vec<Value>> {
    let value = read_json_value(path)?;
    match value {
        Value::Array(items) => Ok(items),
        Value::Object(mut map) => match map.remove("messages") {
            Some(Value::Array(items)) => Ok(items),
            Some(other) => Ok(vec![other]),
            None => Ok(vec![Value::Object(map)]),
        },
        other => Ok(vec![other]),
    }
}

fn count_messages(path: &Path) -> Result<usize> {
    Ok(read_messages(path)?.len())
}

fn channel_title(channel: Option<&Value>, fallback_id: &str) -> String {
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

fn detect_channel_kind(channel: Option<&Value>) -> ChannelKind {
    let Some(channel) = channel else {
        return ChannelKind::Other;
    };

    let raw_type = channel
        .get("type")
        .or_else(|| channel.get("channel_type"))
        .and_then(value_to_plain_string)
        .unwrap_or_else(|| "unknown".to_owned())
        .to_ascii_uppercase();

    if raw_type == "DM" {
        ChannelKind::Dm
    } else if raw_type == "GROUP_DM" {
        ChannelKind::GroupDm
    } else if raw_type.contains("PUBLIC_THREAD") {
        ChannelKind::PublicThread
    } else if raw_type.contains("VOICE") {
        ChannelKind::Voice
    } else if raw_type.starts_with("GUILD") {
        ChannelKind::Guild
    } else {
        ChannelKind::Other
    }
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

fn pick_str<'a>(record: &'a Value, keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        if let Some(Value::String(text)) = record.get(*key) {
            return Some(text);
        }
    }
    None
}
