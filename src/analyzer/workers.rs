use anyhow::Result;
use regex::Regex;
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::UNIX_EPOCH,
};
use unicode_segmentation::UnicodeSegmentation;
use walkdir::WalkDir;

use super::structs::*;
use crate::data::utils::{
    channel_title, extract_attachment_urls, extract_message_content, find_file_case_insensitive,
    pick_plain_string, pick_str, pick_timestamp_month, read_json_value,
    read_records_json_or_ndjson,
};

// Let's go stalk the user's account info. Purely for analytics, I swear!
pub fn analyze_account(account_dir: &Path, stats: &mut AnalysisData) -> Result<()> {
    let user_path = account_dir.join("user.json");
    if !user_path.exists() {
        stats
            .warnings
            .push("Account/user.json missing; user profile summary skipped.".to_owned());
        return Ok(());
    }
    let value = read_json_value(&user_path)?;
    stats.account.user_id = value.get("id").and_then(value_to_plain_string);
    stats.account.username = value
        .get("global_name")
        .and_then(value_to_plain_string)
        .or_else(|| value.get("username").and_then(value_to_plain_string));
    Ok(())
}

pub fn analyze_servers(servers_dir: Option<&Path>, stats: &mut AnalysisData) -> Result<()> {
    let Some(servers_dir) = servers_dir else {
        return Ok(());
    };
    if let Some(index_path) = find_file_case_insensitive(servers_dir, "index.json")? {
        if let Ok(index_value) = read_json_value(&index_path) {
            stats.servers.index_entries = match index_value {
                Value::Array(items) => items.len() as u64,
                Value::Object(map) => map.len() as u64,
                _ => 0,
            };
        }
    }
    for entry in fs::read_dir(servers_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        stats.servers.count += 1;
        let audit_path = path.join("audit-log.json");
        if audit_path.is_file() {
            stats.servers.audit_log_entries += count_json_records(&audit_path)?;
        }
    }
    Ok(())
}

fn count_json_records(path: &Path) -> Result<u64> {
    use crate::data::utils::count_records;
    Ok(count_records(path).unwrap_or(0) as u64)
}

pub fn analyze_support_tickets(support_dir: Option<&Path>, stats: &mut AnalysisData) -> Result<()> {
    let Some(support_dir) = support_dir else {
        return Ok(());
    };
    let Some(tickets_path) = find_file_case_insensitive(support_dir, "tickets.json")? else {
        return Ok(());
    };
    let tickets_value = read_json_value(&tickets_path)?;
    match tickets_value {
        Value::Object(map) => {
            for (_, value) in map {
                summarize_ticket(&value, stats);
            }
        }
        Value::Array(items) => {
            for value in items {
                summarize_ticket(&value, stats);
            }
        }
        _ => {}
    }
    stats.support_tickets.avg_comments_per_ticket = if stats.support_tickets.count > 0 {
        stats.support_tickets.comments as f64 / stats.support_tickets.count as f64
    } else {
        0.0
    };
    Ok(())
}

pub fn summarize_ticket(value: &Value, stats: &mut AnalysisData) {
    if !value.is_object() {
        return;
    }
    stats.support_tickets.count += 1;
    let status = pick_plain_string(value, &["status", "ticket_status", "state"])
        .unwrap_or_else(|| "unknown".to_owned());
    increment_counter(&mut stats.support_tickets.by_status, status, 1);
    if let Some(priority) = pick_plain_string(value, &["priority", "severity", "urgency"]) {
        increment_counter(&mut stats.support_tickets.by_priority, priority, 1);
    }
    if let Some(created_month) = pick_timestamp_month(
        value,
        &[
            "created_at",
            "createdAt",
            "created",
            "opened_at",
            "openedAt",
            "date",
            "timestamp",
        ],
    ) {
        increment_counter(
            &mut stats.support_tickets.by_month,
            created_month.clone(),
            1,
        );
        increment_counter(
            &mut stats.support_tickets.activity_by_month,
            created_month,
            1,
        );
    }
    if let Some(Value::Array(comments)) = value.get("comments") {
        let comment_count = comments.len() as u64;
        stats.support_tickets.comments += comment_count;
        if comment_count > 0 {
            stats.support_tickets.tickets_with_comments += 1;
        }
        for comment in comments {
            if let Some(month) = pick_timestamp_month(
                comment,
                &[
                    "created_at",
                    "createdAt",
                    "date",
                    "timestamp",
                    "updated_at",
                    "updatedAt",
                ],
            ) {
                increment_counter(&mut stats.support_tickets.activity_by_month, month, 1);
            }
        }
    }
    if let Some(month) = pick_timestamp_month(
        value,
        &[
            "updated_at",
            "updatedAt",
            "last_activity_at",
            "lastActivityAt",
            "closed_at",
            "closedAt",
            "resolved_at",
            "resolvedAt",
        ],
    ) {
        increment_counter(&mut stats.support_tickets.activity_by_month, month, 1);
    }
}

// Digging through DMs to find all those cringe messages you sent at 3 AM.
// You know the ones. We all do. Yes...
pub fn analyze_messages(messages_dir: Option<&Path>, stats: &mut AnalysisData) -> Result<()> {
    let Some(messages_dir) = messages_dir else {
        stats
            .warnings
            .push("Messages directory missing; message analysis skipped.".to_owned());
        return Ok(());
    };

    let emoji_re = Regex::new(r"<a?:[A-Za-z0-9_]+:\d+>")?;
    let hour_re = Regex::new(r"(?:T| )(\d{2}):(\d{2}):(\d{2})")?;
    let date_re = Regex::new(r"^(\d{4})-(\d{2})-(\d{2})")?;
    let word_re = Regex::new(r"(?i)\b[a-z]{3,15}\b")?;
    let url_re = Regex::new(r#"https?://[^\s<>"')\]]+"#)?;

    let mut dirs: Vec<PathBuf> = fs::read_dir(messages_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();

    let mut next_cache = BTreeMap::new();
    let mut tasks = Vec::new();
    let mut total_word_freq = HashMap::new();
    let mut total_link_domains: BTreeMap<String, u64> = BTreeMap::new();
    let mut total_char_freq = HashMap::new();
    let mut ch_counts = Vec::new();

    for dir in dirs {
        let id = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_owned();
        let m_path = dir.join("messages.json");
        let c_path = dir.join("channel.json");
        if !m_path.is_file() {
            continue;
        }
        let mt_m = get_mtime_ms(&m_path);
        let mt_c = get_mtime_ms(&c_path);

        if let Some(cached) = stats.channels_cache.get(&id) {
            if cached.cache_version == crate::analyzer::structs::CHANNEL_CACHE_VERSION
                && cached.mtime_messages == mt_m
                && cached.mtime_channel == mt_c
            {
                stats.messages.channels += 1;
                stats.messages.total += cached.message_count;
                stats.messages.with_content += cached.messages_with_content;
                stats.messages.with_attachments += cached.attachment_count;
                stats
                    .messages
                    .attachment_links
                    .extend(cached.attachment_links.clone());
                increment_counter(&mut stats.messages.by_channel_type, &cached.channel_type, 1);
                stats.messages.temporal.merge(&cached.temporal);
                stats.messages.content.merge(&cached.content);
                for (w, c) in &cached.word_frequency {
                    *total_word_freq.entry(w.clone()).or_insert(0) += c;
                }
                for (ch, c) in &cached.content.character_frequency {
                    *total_char_freq.entry(*ch).or_insert(0) += c;
                }
                stats.insights.links.messages_with_links += cached.link_messages;
                stats.insights.links.total_links += cached.total_links;
                stats.insights.links.question_messages += cached.question_messages;
                for (d, c) in &cached.link_domains {
                    *total_link_domains.entry(d.clone()).or_insert(0) += c;
                }
                ch_counts.push((cached.channel_title.clone(), cached.message_count));
                next_cache.insert(id, cached.clone());
                continue;
            }
        }
        tasks.push(ChannelTask {
            id,
            messages_path: m_path,
            channel_path: c_path,
            mtime_messages: mt_m,
            mtime_channel: mt_c,
        });
    }

    // We have multiple workers parsing thousands of files. My CPU temperature is approaching that of the sun.
    if !tasks.is_empty() {
        let worker_count = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .min(tasks.len())
            .max(1);
        let queue = Arc::new(Mutex::new(VecDeque::from(tasks)));
        let (tx, rx) = mpsc::channel::<ChannelWorkerEvent>();
        for _ in 0..worker_count {
            let q = Arc::clone(&queue);
            let wtx = tx.clone();
            let ere = emoji_re.clone();
            let hre = hour_re.clone();
            let dre = date_re.clone();
            let wre = word_re.clone();
            let url_re = url_re.clone();
            thread::spawn(move || {
                while let Some(task) = {
                    let mut g = q.lock().unwrap();
                    g.pop_front()
                } {
                    let mut cstats = ChannelAnalysisCache {
                        cache_version: crate::analyzer::structs::CHANNEL_CACHE_VERSION,
                        mtime_messages: task.mtime_messages,
                        mtime_channel: task.mtime_channel,
                        ..Default::default()
                    };
                    if let Ok(val) = read_json_value(&task.channel_path) {
                        cstats.channel_type = val
                            .get("type")
                            .and_then(value_to_plain_string)
                            .unwrap_or_else(|| "unknown".to_owned());
                        cstats.channel_title = channel_title(Some(&val), &task.id);
                    } else {
                        cstats.channel_title = task.id.clone();
                    }

                    if let Ok(records) = read_records_json_or_ndjson(&task.messages_path) {
                        for rec in records {
                            cstats.message_count += 1;
                            let content = extract_message_content(&rec);
                            let attachments = extract_attachment_urls(&rec);
                            let msg_ts: Option<String> =
                                pick_str(&rec, &["Timestamp", "timestamp", "timestamp_ms", "date"])
                                    .map(str::to_owned);
                            if let Some(ts) = &msg_ts {
                                if let Some(caps) = hre.captures(ts) {
                                    if let Ok(hr) = caps[1].parse::<u32>() {
                                        *cstats.temporal.by_hour.entry(hr).or_insert(0) += 1;
                                    }
                                }
                                if let Some(caps) = dre.captures(ts) {
                                    let (y, m, d) = (
                                        caps[1].parse::<u32>().unwrap_or(0),
                                        caps[2].parse::<u32>().unwrap_or(0),
                                        caps[3].parse::<u32>().unwrap_or(0),
                                    );
                                    if (1..=12).contains(&m) {
                                        *cstats.temporal.by_month.entry(m).or_insert(0) += 1;
                                    }
                                    if y >= 1 && m >= 1 && d >= 1 {
                                        *cstats
                                            .temporal
                                            .by_day_of_week
                                            .entry(day_of_week(y, m, d))
                                            .or_insert(0) += 1;
                                    }
                                    let ds = format!("{y:04}-{m:02}-{d:02}");
                                    *cstats.temporal.by_day.entry(ds.clone()).or_insert(0) += 1;
                                    if cstats
                                        .temporal
                                        .first_message_date
                                        .as_deref()
                                        .is_none_or(|f| ds < f.to_owned())
                                    {
                                        cstats.temporal.first_message_date = Some(ds.clone());
                                    }
                                    if cstats
                                        .temporal
                                        .last_message_date
                                        .as_deref()
                                        .is_none_or(|l| ds > l.to_owned())
                                    {
                                        cstats.temporal.last_message_date = Some(ds);
                                    }
                                }
                            }
                            if !content.is_empty() {
                                cstats.messages_with_content += 1;
                                let content_len = content.chars().count() as u64;
                                cstats.content.total_chars += content_len;
                                if content_len > cstats.max_message_chars {
                                    cstats.max_message_chars = content_len;
                                    cstats.max_message_date =
                                        msg_ts.as_deref().map(|t| t.chars().take(10).collect());
                                }
                                cstats.content.linebreaks += content.matches('\n').count() as u64;
                                cstats.content.emoji_custom +=
                                    ere.find_iter(&content).count() as u64;
                                for g in content.graphemes(true) {
                                    if emojis::get(g).is_some() {
                                        cstats.content.emoji_unicode += 1;
                                    }
                                }
                                for ch in content.chars() {
                                    *cstats.content.character_frequency.entry(ch).or_insert(0) += 1;
                                }
                                for mat in wre.find_iter(&content.to_ascii_lowercase()) {
                                    if !is_stop_word(mat.as_str()) {
                                        *cstats
                                            .word_frequency
                                            .entry(mat.as_str().to_owned())
                                            .or_insert(0) += 1;
                                    }
                                }
                                // Link / question intelligence for the Insights screen.
                                if content.contains('?') {
                                    cstats.question_messages += 1;
                                }
                                let mut links_in_msg = 0u64;
                                for url in url_re.find_iter(&content) {
                                    cstats.total_links += 1;
                                    links_in_msg += 1;
                                    if let Some(host) = extract_host(url.as_str()) {
                                        *cstats.link_domains.entry(host).or_insert(0) += 1;
                                    }
                                }
                                if links_in_msg > 0 {
                                    cstats.link_messages += 1;
                                }
                            }
                            for url in attachments {
                                if url.starts_with("https://cdn.discordapp.com/attachments/") {
                                    cstats.attachment_links.push(url);
                                }
                            }
                            cstats.attachment_count = cstats.attachment_links.len() as u64;
                        }
                        let _ = wtx.send(ChannelWorkerEvent::Finished {
                            id: task.id,
                            stats: cstats,
                        });
                    } else {
                        let _ = wtx.send(ChannelWorkerEvent::Failed {
                            id: task.id,
                            error: "Read failed".to_owned(),
                        });
                    }
                }
            });
        }
        drop(tx);
        while let Ok(event) = rx.recv() {
            match event {
                ChannelWorkerEvent::Finished { id, stats: c_entry } => {
                    stats.messages.channels += 1;
                    stats.messages.total += c_entry.message_count;
                    stats.messages.with_content += c_entry.messages_with_content;
                    stats.messages.with_attachments += c_entry.attachment_count;
                    stats
                        .messages
                        .attachment_links
                        .extend(c_entry.attachment_links.clone());
                    increment_counter(
                        &mut stats.messages.by_channel_type,
                        &c_entry.channel_type,
                        1,
                    );
                    stats.messages.temporal.merge(&c_entry.temporal);
                    stats.messages.content.merge(&c_entry.content);
                    for (w, c) in &c_entry.word_frequency {
                        *total_word_freq.entry(w.clone()).or_insert(0) += c;
                    }
                    for (ch, c) in &c_entry.content.character_frequency {
                        *total_char_freq.entry(*ch).or_insert(0) += c;
                    }
                    stats.insights.links.messages_with_links += c_entry.link_messages;
                    stats.insights.links.total_links += c_entry.total_links;
                    stats.insights.links.question_messages += c_entry.question_messages;
                    for (d, c) in &c_entry.link_domains {
                        *total_link_domains.entry(d.clone()).or_insert(0) += c;
                    }
                    ch_counts.push((c_entry.channel_title.clone(), c_entry.message_count));
                    next_cache.insert(id, c_entry);
                }
                ChannelWorkerEvent::Failed { id, error } => {
                    stats.warnings.push(format!("Channel {id}: {error}"));
                }
            }
        }
    }
    stats.channels_cache = next_cache;
    stats.messages.content.distinct_characters = total_char_freq.len();
    stats.messages.content.avg_length_chars = if stats.messages.with_content > 0 {
        stats.messages.content.total_chars as f64 / stats.messages.with_content as f64
    } else {
        0.0
    };
    let mut words: Vec<_> = total_word_freq.into_iter().collect();
    words.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    words.truncate(100);
    stats.messages.content.top_words = words;
    ch_counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ch_counts.truncate(25);
    stats.messages.top_channels = ch_counts;

    let mut domains: Vec<(String, u64)> = total_link_domains.into_iter().collect();
    domains.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    domains.truncate(20);
    stats.insights.links.top_domains = domains;

    // Personal records for the Insights screen.
    let records = &mut stats.insights.records;
    records.first_message_date = stats.messages.temporal.first_message_date.clone();
    records.last_message_date = stats.messages.temporal.last_message_date.clone();
    records.biggest_channel = stats.messages.top_channels.first().cloned();
    for cached in stats.channels_cache.values() {
        if cached.max_message_chars > records.longest_message_chars {
            records.longest_message_chars = cached.max_message_chars;
            records.longest_message_date = cached.max_message_date.clone();
        }
    }
    Ok(())
}

/// Pull the host out of an http(s) URL, lowercased, without the www.
fn extract_host(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() {
        return None;
    }
    let host = host.strip_prefix("www.").unwrap_or(host);
    Some(host.to_ascii_lowercase())
}

pub fn analyze_activities(activities_dir: Option<&Path>, stats: &mut AnalysisData) -> Result<()> {
    let Some(activities_dir) = activities_dir else {
        return Ok(());
    };
    for entry in WalkDir::new(activities_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        stats.activities.files += 1;
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name == "favorite_games.json" {
            if let Ok(v) = read_json_value(entry.path()) {
                stats.activities.favorite_games = v
                    .get("favorite_games")
                    .and_then(|v| v.as_u64())
                    .or(stats.activities.favorite_games);
            }
        } else if name == "preferences.json" {
            if let Ok(Value::Array(items)) = read_json_value(entry.path()) {
                stats.activities.preferences_entries += items.len() as u64;
            }
        } else if name == "user_data.json" {
            if let Ok(Value::Object(map)) = read_json_value(entry.path()) {
                stats.activities.user_data_apps += map.len() as u64;
            }
        }
    }
    Ok(())
}

pub fn analyze_programs(programs_dir: Option<&Path>, stats: &mut AnalysisData) -> Result<()> {
    let Some(programs_dir) = programs_dir else {
        return Ok(());
    };
    stats.programs.files = WalkDir::new(programs_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .count() as u64;
    Ok(())
}

struct ChannelTask {
    id: String,
    messages_path: PathBuf,
    channel_path: PathBuf,
    mtime_messages: u64,
    mtime_channel: u64,
}
enum ChannelWorkerEvent {
    Finished {
        id: String,
        stats: ChannelAnalysisCache,
    },
    Failed {
        id: String,
        error: String,
    },
}

pub fn increment_counter(map: &mut BTreeMap<String, u64>, key: impl Into<String>, by: u64) {
    *map.entry(key.into()).or_insert(0) += by;
}

pub fn get_mtime_ms(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// Oh magic math algorithms from the 90s, please give me the day of the week!
fn day_of_week(year: u32, month: u32, day: u32) -> u32 {
    const T: [u32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    let dow = (y + y / 4 - y / 100 + y / 400 + T[(month - 1) as usize] + day) % 7;
    if dow == 0 { 6 } else { dow - 1 }
}

fn is_stop_word(w: &str) -> bool {
    matches!(
        w,
        "the"
            | "and"
            | "you"
            | "that"
            | "was"
            | "for"
            | "are"
            | "with"
            | "his"
            | "they"
            | "this"
            | "have"
            | "from"
            | "one"
            | "had"
            | "word"
            | "but"
            | "not"
            | "what"
            | "all"
            | "were"
            | "when"
            | "your"
            | "can"
            | "said"
            | "there"
            | "use"
            | "each"
            | "which"
            | "she"
            | "how"
            | "their"
            | "will"
            | "other"
            | "about"
            | "out"
            | "many"
            | "then"
            | "them"
            | "these"
            | "some"
            | "her"
            | "would"
            | "make"
            | "like"
            | "him"
            | "into"
            | "time"
            | "has"
            | "look"
            | "two"
            | "more"
            | "write"
            | "see"
            | "number"
            | "way"
            | "could"
            | "people"
            | "than"
            | "first"
            | "water"
            | "been"
            | "call"
            | "who"
            | "oil"
            | "its"
            | "now"
            | "find"
            | "long"
            | "down"
            | "day"
            | "did"
            | "get"
            | "come"
            | "made"
            | "may"
            | "part"
            | "https"
            | "http"
            | "com"
            | "www"
            | "net"
            | "org"
    )
}

pub fn value_to_plain_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}
