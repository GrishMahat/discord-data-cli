use anyhow::Result;
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, VecDeque},
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Sender},
        Arc, Mutex,
    },
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

// Are you feeling chatty? Or maybe you're just screaming into the void?
// 900 files taking 50 terabytes? Oh my god... YES. Let's parse 'em all!
pub fn analyze_activity<F>(
    activity_dir: Option<&Path>,
    stats: &mut AnalysisData,
    mut on_progress: F,
) -> Result<()>
where
    F: FnMut(f32, String),
{
    let Some(activity_dir) = activity_dir else {
        return Ok(());
    };
    let mut files: Vec<PathBuf> = WalkDir::new(activity_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| {
            p.extension()
                .map(|ext| ext.to_string_lossy().eq_ignore_ascii_case("json"))
                .unwrap_or(false)
        })
        .collect();
    files.sort();
    if files.is_empty() {
        return Ok(());
    }

    let mut next_activity_cache = BTreeMap::new();
    let mut tasks = Vec::new();
    let mut tasks_paths = Vec::new();
    for path in files {
        let mtime = get_mtime_ms(&path);
        let rel_path = path
            .strip_prefix(activity_dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        if let Some(cached) = stats.activity_cache.get(&rel_path) {
            if cached.mtime_ms == mtime {
                stats.activity.files += 1;
                stats.activity.total_events += cached.stats.event_lines;
                stats.activity.parse_errors += cached.stats.parse_errors;
                for (et, c) in &cached.stats.event_types {
                    increment_counter(&mut stats.activity.by_event_type, et, *c);
                }
                next_activity_cache.insert(rel_path, cached.clone());
                continue;
            }
        }
        let index = tasks.len();
        let size = fs::metadata(&path).map(|m| m.len().max(1)).unwrap_or(1);
        let short_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown.json")
            .to_owned();
        tasks.push(ActivityFileTask {
            index,
            path: path.clone(),
            size,
            short_name,
        });
        tasks_paths.push((index, rel_path, mtime));
    }
    if tasks.is_empty() {
        stats.activity_cache = next_activity_cache;
        return Ok(());
    }

    stats.activity.files += tasks.len() as u64;
    let total_tasks = tasks.len();
    let file_sizes: Vec<u64> = tasks.iter().map(|t| t.size).collect();
    let file_names: Vec<String> = tasks.iter().map(|t| t.short_name.clone()).collect();
    let total_bytes: u64 = file_sizes.iter().sum::<u64>().max(1);
    let mut total_bytes_read = 0_u64;
    let mut file_bytes_read = vec![0_u64; total_tasks];

    let worker_count = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(total_tasks)
        .max(1);
    let task_queue = Arc::new(Mutex::new(VecDeque::from(tasks)));
    let (tx, rx) = mpsc::channel::<ActivityWorkerEvent>();
    let mut handles = Vec::new();
    for _ in 0..worker_count {
        let queue = Arc::clone(&task_queue);
        let worker_tx = tx.clone();
        handles.push(thread::spawn(move || {
            while let Some(task) = {
                let mut q = queue.lock().unwrap();
                q.pop_front()
            } {
                match process_activity_file(&task, &worker_tx) {
                    Ok(fstats) => {
                        let _ = worker_tx.send(ActivityWorkerEvent::Finished {
                            file_index: task.index,
                            stats: fstats,
                        });
                    }
                    Err(err) => {
                        let _ = worker_tx.send(ActivityWorkerEvent::Failed {
                            _file_index: task.index,
                            error: err.to_string(),
                        });
                    }
                }
            }
        }));
    }
    drop(tx);

    let mut finished = 0;
    while finished < total_tasks {
        match rx.recv()? {
            ActivityWorkerEvent::Progress {
                file_index,
                bytes_read,
            } => {
                let capped = bytes_read.min(file_sizes[file_index]);
                if capped > file_bytes_read[file_index] {
                    total_bytes_read =
                        total_bytes_read.saturating_add(capped - file_bytes_read[file_index]);
                    file_bytes_read[file_index] = capped;
                }
                on_progress(
                    total_bytes_read as f32 / total_bytes as f32,
                    format!(
                        "file {}/{}: {}",
                        file_index + 1,
                        total_tasks,
                        file_names[file_index]
                    ),
                );
            }
            ActivityWorkerEvent::Finished {
                file_index,
                stats: fstats,
            } => {
                if file_sizes[file_index] > file_bytes_read[file_index] {
                    total_bytes_read = total_bytes_read
                        .saturating_add(file_sizes[file_index] - file_bytes_read[file_index]);
                    file_bytes_read[file_index] = file_sizes[file_index];
                }
                stats.activity.total_events += fstats.event_lines;
                stats.activity.parse_errors += fstats.parse_errors;
                for (et, c) in &fstats.event_types {
                    increment_counter(&mut stats.activity.by_event_type, et, *c);
                }
                if let Some((_, rel_path, mtime)) =
                    tasks_paths.iter().find(|(idx, _, _)| *idx == file_index)
                {
                    next_activity_cache.insert(
                        rel_path.clone(),
                        ActivityFileCache {
                            mtime_ms: *mtime,
                            stats: fstats,
                        },
                    );
                }
                finished += 1;
                on_progress(
                    total_bytes_read as f32 / total_bytes as f32,
                    format!(
                        "file {}/{}: {} complete",
                        file_index + 1,
                        total_tasks,
                        file_names[file_index]
                    ),
                );
            }
            ActivityWorkerEvent::Failed { _file_index, error } => {
                finished += 1;
                stats.warnings.push(error);
            }
        }
    }
    for h in handles {
        let _ = h.join();
    }
    stats.activity_cache = next_activity_cache;
    Ok(())
}

// Here's where the real magic happens. We read 900 GB of JSON line by line.
// Pray for our RAM.
fn process_activity_file(
    task: &ActivityFileTask,
    tx: &Sender<ActivityWorkerEvent>,
) -> Result<ActivityFileStats> {
    const REPORT_INTERVAL: u64 = 8 * 1024 * 1024;
    let file = File::open(&task.path)?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut line = Vec::new();
    let mut bytes_read = 0u64;
    let mut next_report = REPORT_INTERVAL;
    let mut fstats = ActivityFileStats::default();

    while reader.read_until(b'\n', &mut line)? > 0 {
        bytes_read += line.len() as u64;
        if bytes_read >= next_report {
            let _ = tx.send(ActivityWorkerEvent::Progress {
                file_index: task.index,
                bytes_read,
            });
            next_report += REPORT_INTERVAL;
        }
        let trimmed = trim_ascii_whitespace(&line);
        if !trimmed.is_empty() {
            fstats.event_lines += 1;
            if let Ok(value) = serde_json::from_slice::<ActivityEventLine>(trimmed) {
                increment_counter(
                    &mut fstats.event_types,
                    value.event_type.unwrap_or(Cow::Borrowed("unknown")),
                    1,
                );
            } else {
                fstats.parse_errors += 1;
            }
        }
        line.clear();
    }
    Ok(fstats)
}

#[derive(Debug, Deserialize)]
struct ActivityEventLine<'a> {
    #[serde(borrow, default)]
    event_type: Option<Cow<'a, str>>,
}

struct ActivityFileTask {
    index: usize,
    path: PathBuf,
    size: u64,
    short_name: String,
}

enum ActivityWorkerEvent {
    Progress {
        file_index: usize,
        bytes_read: u64,
    },
    Finished {
        file_index: usize,
        stats: ActivityFileStats,
    },
    Failed {
        _file_index: usize,
        error: String,
    },
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

    let mut dirs: Vec<PathBuf> = fs::read_dir(messages_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();

    let mut next_cache = BTreeMap::new();
    let mut tasks = Vec::new();
    let mut total_word_freq = HashMap::new();
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
            if cached.mtime_messages == mt_m && cached.mtime_channel == mt_c {
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
            thread::spawn(move || {
                while let Some(task) = {
                    let mut g = q.lock().unwrap();
                    g.pop_front()
                } {
                    let mut cstats = ChannelAnalysisCache {
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
                            if let Some(ts) =
                                pick_str(&rec, &["Timestamp", "timestamp", "timestamp_ms", "date"])
                            {
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
                                cstats.content.total_chars += content.chars().count() as u64;
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
    Ok(())
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
    if dow == 0 {
        6
    } else {
        dow - 1
    }
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

fn trim_ascii_whitespace(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .map(|idx| idx + 1)
        .unwrap_or(start);
    &bytes[start..end]
}

pub fn value_to_plain_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}
