use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde_json::Value;

use crate::config::SourceAliases;
use crate::data::utils::{
    channel_title, count_records, pick_str, read_json_value, read_records_json_or_ndjson,
    resolve_optional_subdir, value_to_plain_string,
};

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
    let messages = read_records_json_or_ndjson(&channel.messages_path)?;
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

fn count_messages(path: &Path) -> Result<usize> {
    count_records(path)
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
