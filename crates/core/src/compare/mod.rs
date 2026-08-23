//! Phase 3: "What changed since my last export?" — point-in-time snapshots of
//! headline stats plus a diff view between any two consecutive analyses.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::analyzer::AnalysisData;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Snapshot {
    pub taken_at: String,
    pub messages_total: u64,
    pub channels: u64,
    pub servers: u64,
    pub dm_channels: u64,
    pub dm_messages: u64,
    pub group_channels: u64,
    pub group_messages: u64,
    pub top_channel: Option<(String, u64)>,
    pub top_contact: Option<(String, u64)>,
    pub top_word: Option<(String, u64)>,
    pub links_total: u64,
    pub question_messages: u64,
    pub voice_connections: u64,
    /// Whole minutes across all sessions.
    pub voice_minutes: u64,
    pub active_days: u64,
    pub longest_streak_days: u64,
    pub first_message: Option<String>,
    pub last_message: Option<String>,
}

impl Snapshot {
    pub fn from_data(data: &AnalysisData) -> Self {
        let ins = &data.insights;
        Self {
            taken_at: data.meta.analyzed_at.clone(),
            messages_total: data.messages.total,
            channels: data.messages.channels,
            servers: data.servers.count,
            dm_channels: ins.social.dm_channels,
            dm_messages: ins.social.dm_messages,
            group_channels: ins.social.group_dm_channels,
            group_messages: ins.social.group_dm_messages,
            top_channel: data.messages.top_channels.first().cloned(),
            top_contact: ins
                .social
                .top_contacts
                .first()
                .map(|c| (c.name.clone(), c.messages)),
            top_word: data.messages.content.top_words.first().cloned(),
            links_total: ins.links.total_links,
            question_messages: ins.links.question_messages,
            voice_connections: ins.voice.connections,
            voice_minutes: ins.voice.connected_minutes as u64,
            active_days: ins.sessions.active_days,
            longest_streak_days: ins.sessions.longest_daily_streak,
            first_message: data.messages.temporal.first_message_date.clone(),
            last_message: data.messages.temporal.last_message_date.clone(),
        }
    }
}

fn snapshots_dir(results_dir: &Path) -> std::path::PathBuf {
    results_dir.join("snapshots")
}

/// Persist a snapshot for this analysis run. Skips the write when the newest
/// existing snapshot has identical stats (re-running analysis unchanged);
/// `taken_at` is excluded from that comparison since it always differs.
pub fn write_snapshot(results_dir: &Path, data: &AnalysisData) -> Result<()> {
    let dir = snapshots_dir(results_dir);
    fs::create_dir_all(&dir)?;
    let mut snapshot = Snapshot::from_data(data);
    let stamp = data
        .meta
        .analyzed_at
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>();

    // Skip duplicates: same stats as the most recent file.
    let mut existing: Vec<_> = fs::read_dir(&dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    existing.sort();
    if let Some(latest) = existing.last() {
        let fresh = fs::read_to_string(latest).ok().and_then(|c| {
            let mut prev: Snapshot = serde_json::from_str(&c).ok()?;
            prev.taken_at = String::new();
            Some(prev)
        });
        snapshot.taken_at = String::new();
        if fresh.is_some_and(|prev| prev == snapshot) {
            return Ok(());
        }
        snapshot.taken_at = data.meta.analyzed_at.clone();
    }

    let path = dir.join(format!("snapshot-{stamp}.json"));
    let body = serde_json::to_string_pretty(&snapshot)?;
    fs::write(&path, body).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// All snapshots, newest first: (filename stem, snapshot).
pub fn list_snapshots(results_dir: &Path) -> Vec<(String, Snapshot)> {
    let dir = snapshots_dir(results_dir);
    let mut out: Vec<(String, Snapshot)> = fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "json") {
                return None;
            }
            let body = fs::read_to_string(&path).ok()?;
            let snap: Snapshot = serde_json::from_str(&body).ok()?;
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .trim_start_matches("snapshot-")
                .to_owned();
            Some((stem, snap))
        })
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out
}

/// One row of the comparison table.
pub struct DiffRow {
    pub label: &'static str,
    pub old: String,
    pub new: String,
    pub delta: String,
    /// true = growth (green), false = decline (red), None = flat / n/a
    pub trend: Option<bool>,
}

fn fmt_opt_pair(name: &str, v: &Option<(String, u64)>) -> String {
    match v {
        Some((n, c)) => format!("{n} ({c})"),
        None => format!("{name} n/a"),
    }
}

pub fn diff_snapshots(old: &Snapshot, new: &Snapshot) -> Vec<DiffRow> {
    let mut rows = Vec::new();

    let mut push_count = |label: &'static str, o: u64, n: u64| {
        let d = n as i64 - o as i64;
        rows.push(DiffRow {
            label,
            old: o.to_string(),
            new: n.to_string(),
            delta: signed(d),
            trend: if d == 0 { None } else { Some(d > 0) },
        });
    };

    push_count("Messages", old.messages_total, new.messages_total);
    push_count("Channels", old.channels, new.channels);
    push_count("Servers", old.servers, new.servers);
    push_count("DM channels", old.dm_channels, new.dm_channels);
    push_count("DM messages", old.dm_messages, new.dm_messages);
    push_count("Group DMs", old.group_channels, new.group_channels);
    push_count("Group messages", old.group_messages, new.group_messages);
    push_count("Links shared", old.links_total, new.links_total);
    push_count(
        "Questions asked",
        old.question_messages,
        new.question_messages,
    );
    push_count(
        "Voice connections",
        old.voice_connections,
        new.voice_connections,
    );
    push_count("Voice minutes", old.voice_minutes, new.voice_minutes);
    push_count("Active days", old.active_days, new.active_days);

    // Named "top" entries: show changes in value/count.
    rows.push(DiffRow {
        label: "Top channel",
        old: fmt_opt_pair("", &old.top_channel),
        new: fmt_opt_pair("", &new.top_channel),
        delta: changed_marker(
            old.top_channel.as_ref().map(|(n, _)| n.as_str()),
            new.top_channel.as_ref().map(|(n, _)| n.as_str()),
        ),
        trend: None,
    });
    rows.push(DiffRow {
        label: "Top contact",
        old: fmt_opt_pair("", &old.top_contact),
        new: fmt_opt_pair("", &new.top_contact),
        delta: changed_marker(
            old.top_contact.as_ref().map(|(n, _)| n.as_str()),
            new.top_contact.as_ref().map(|(n, _)| n.as_str()),
        ),
        trend: None,
    });
    rows.push(DiffRow {
        label: "Top word",
        old: old
            .top_word
            .as_ref()
            .map(|(w, c)| format!("{w} ({c})"))
            .unwrap_or_else(|| "n/a".to_owned()),
        new: new
            .top_word
            .as_ref()
            .map(|(w, c)| format!("{w} ({c})"))
            .unwrap_or_else(|| "n/a".to_owned()),
        delta: String::new(),
        trend: None,
    });
    rows.push(DiffRow {
        label: "Last message",
        old: old.last_message.clone().unwrap_or_else(|| "n/a".to_owned()),
        new: new.last_message.clone().unwrap_or_else(|| "n/a".to_owned()),
        delta: String::new(),
        trend: None,
    });

    rows
}

fn signed(d: i64) -> String {
    if d > 0 {
        format!("+{d}")
    } else if d < 0 {
        d.to_string()
    } else {
        "=".to_owned()
    }
}

fn changed_marker(old: Option<&str>, new: Option<&str>) -> String {
    match (old, new) {
        (Some(o), Some(n)) if o == n => "same".to_owned(),
        _ => "changed".to_owned(),
    }
}
