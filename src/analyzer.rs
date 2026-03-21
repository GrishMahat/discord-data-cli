use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, VecDeque},
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{self, Sender},
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use unicode_segmentation::UnicodeSegmentation;
use walkdir::WalkDir;

use crate::config::{AppConfig, SourceAliases};
use crate::data::utils::{
    channel_title, extract_attachment_urls, extract_message_content, find_file_case_insensitive,
    pick_plain_string, pick_str, pick_timestamp_month, read_json_value, read_records_json_or_ndjson,
    resolve_optional_subdir, value_to_plain_string,
};

// ── Top-level output ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AnalysisData {
    /// Tool metadata (version, when the analysis ran, paths used).
    pub meta: Meta,
    /// Basic Discord account information.
    pub account: Account,
    /// Which data folders were present in the export.
    pub folder_presence: BTreeMap<String, bool>,
    /// Non-fatal issues encountered during analysis.
    pub warnings: Vec<String>,
    /// Everything related to messages you have sent.
    pub messages: Messages,
    /// Server membership data.
    pub servers: Servers,
    /// Discord support tickets.
    pub support_tickets: SupportTickets,
    /// Raw activity event logs.
    pub activity: Activity,
    /// Activities / gaming data.
    pub activities: Activities,
    /// Installed programs detected in the export.
    pub programs: Programs,

    /// Legacy flat fields kept for backward compat (hidden from pretty JSON)
    #[serde(skip)]
    pub package_directory: String,
    #[serde(skip)]
    pub results_directory: String,

    /// Cache for incremental analysis (hidden from pretty JSON)
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub channels_cache: BTreeMap<String, ChannelAnalysisCache>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub activity_cache: BTreeMap<String, ActivityFileCache>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelAnalysisCache {
    pub mtime_messages: u64,
    pub mtime_channel: u64,
    pub message_count: u64,
    pub messages_with_content: u64,
    pub channel_type: String,
    pub channel_title: String,
    pub temporal: Temporal,
    pub content: ContentStats,
    pub word_frequency: BTreeMap<String, u64>,
    pub attachment_count: u64,
    pub attachment_links: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActivityFileCache {
    pub mtime_ms: u64,
    pub stats: ActivityFileStats,
}

// ── Meta ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Meta {
    /// Semver of this tool.
    pub tool_version: String,
    /// ISO-8601 UTC timestamp of when this analysis was run.
    pub analyzed_at: String,
    /// Absolute path to the Discord export package.
    pub package_directory: String,
    /// Absolute path where results are written.
    pub results_directory: String,
}

// ── Account ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Account {
    pub user_id: Option<String>,
    pub username: Option<String>,
}

// ── Messages ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Messages {
    /// Total messages found across all channels.
    pub total: u64,
    /// Number of channels that contained messages.
    pub channels: u64,
    /// Messages split by channel type (DM, GROUP_DM, GUILD_TEXT, …).
    pub by_channel_type: BTreeMap<String, u64>,
    /// Number of messages that had any text content.
    pub with_content: u64,
    /// Number of messages that had file/image attachments.
    pub with_attachments: u64,
    /// CDN URLs of attachments (for the downloader).
    pub attachment_links: Vec<String>,
    /// Content statistics.
    pub content: ContentStats,
    /// Temporal distribution.
    pub temporal: Temporal,
    /// Top channels ranked by message count (up to 25).
    pub top_channels: Vec<(String, u64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContentStats {
    pub distinct_characters: usize,
    pub character_frequency: BTreeMap<String, u64>,
    pub top_words: Vec<(String, u64)>,
    pub emoji_unicode: u64,
    pub emoji_custom: u64,
    pub linebreaks: u64,
    pub avg_length_chars: f64,
    pub total_chars: u64,
}

impl ContentStats {
    pub(crate) fn merge(&mut self, other: &Self) {
        self.total_chars += other.total_chars;
        self.linebreaks += other.linebreaks;
        self.emoji_custom += other.emoji_custom;
        self.emoji_unicode += other.emoji_unicode;

        for (ch, count) in &other.character_frequency {
            *self.character_frequency.entry(ch.clone()).or_insert(0) += count;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Temporal {
    pub first_message_date: Option<String>,
    pub last_message_date: Option<String>,
    pub by_hour: BTreeMap<u32, u64>,
    pub by_day_of_week: BTreeMap<u32, u64>,
    pub by_month: BTreeMap<u32, u64>,
}

impl Temporal {
    pub(crate) fn merge(&mut self, other: &Self) {
        if self.first_message_date.is_none()
            || (other.first_message_date.is_some()
                && other.first_message_date.as_ref().unwrap()
                    < self.first_message_date.as_ref().unwrap())
        {
            self.first_message_date = other.first_message_date.clone();
        }

        if self.last_message_date.is_none()
            || (other.last_message_date.is_some()
                && other.last_message_date.as_ref().unwrap()
                    > self.last_message_date.as_ref().unwrap())
        {
            self.last_message_date = other.last_message_date.clone();
        }

        for (h, c) in &other.by_hour {
            *self.by_hour.entry(*h).or_insert(0) += c;
        }
        for (d, c) in &other.by_day_of_week {
            *self.by_day_of_week.entry(*d).or_insert(0) += c;
        }
        for (m, c) in &other.by_month {
            *self.by_month.entry(*m).or_insert(0) += c;
        }
    }
}

// ── Servers ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Servers {
    /// Number of server directories in the export.
    pub count: u64,
    /// Number of entries in the server index file.
    pub index_entries: u64,
    /// Total audit-log entries across all servers.
    pub audit_log_entries: u64,
}

// ── Support Tickets ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SupportTickets {
    pub count: u64,
    pub comments: u64,
    pub tickets_with_comments: u64,
    pub avg_comments_per_ticket: f64,
    /// Ticket counts grouped by status (open, closed, …).
    pub by_status: BTreeMap<String, u64>,
    /// Ticket counts grouped by priority/severity.
    pub by_priority: BTreeMap<String, u64>,
    /// Ticket creation counts by month (YYYY-MM).
    pub by_month: BTreeMap<String, u64>,
    /// Support-ticket activity events by month (created/comments/updates).
    pub activity_by_month: BTreeMap<String, u64>,
}

// ── Activity ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Activity {
    pub files: u64,
    pub total_events: u64,
    pub parse_errors: u64,
    /// Event counts grouped by event_type string.
    pub by_event_type: BTreeMap<String, u64>,
}

// ── Activities / gaming ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Activities {
    pub files: u64,
    pub preferences_entries: u64,
    pub user_data_apps: u64,
    pub favorite_games: Option<u64>,
}

// ── Programs ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Programs {
    pub files: u64,
}

// ── Progress ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AnalysisProgress {
    pub fraction: f32,
    pub label: String,
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn run_with_progress<F>(
    config: &AppConfig,
    config_path: &Path,
    id: &str,
    mut on_progress: F,
) -> Result<AnalysisData>
where
    F: FnMut(AnalysisProgress),
{
    const TOTAL_STEPS: f32 = 9.0;
    let step_fraction = |step: f32| (step / TOTAL_STEPS).clamp(0.0, 1.0);

    emit_progress(
        &mut on_progress,
        step_fraction(0.0),
        "Preparing analysis...",
    );

    let package_dir = config.package_path(config_path, id);
    if !package_dir.exists() {
        bail!(
            "package_directory does not exist: {}",
            package_dir.display()
        );
    }

    let results_dir = config.results_path(config_path, id);
    fs::create_dir_all(&results_dir)
        .with_context(|| format!("failed to create {}", results_dir.display()))?;

    let source_dirs = SourceDirs::discover(&package_dir, &config.source_aliases)?;

    let existing_data = read_data(&results_dir).ok().flatten();
    let mut stats = AnalysisData {
        meta: Meta {
            tool_version: env!("CARGO_PKG_VERSION").to_owned(),
            analyzed_at: utc_now_iso8601(),
            package_directory: package_dir.display().to_string(),
            results_directory: results_dir.display().to_string(),
        },
        folder_presence: source_dirs.presence_map(),
        package_directory: package_dir.display().to_string(),
        results_directory: results_dir.display().to_string(),
        channels_cache: existing_data.as_ref()
            .map(|d| d.channels_cache.clone())
            .unwrap_or_default(),
        activity_cache: existing_data.as_ref()
            .map(|d| d.activity_cache.clone())
            .unwrap_or_default(),
        ..AnalysisData::default()
    };

    emit_progress(&mut on_progress, step_fraction(1.0), "Analyzing account...");
    if let Some(account_dir) = &source_dirs.account {
        analyze_account(account_dir, &mut stats)?;
    } else {
        stats
            .warnings
            .push("Account directory missing; user profile summary skipped.".to_owned());
    }

    emit_progress(
        &mut on_progress,
        step_fraction(2.0),
        "Analyzing messages...",
    );
    analyze_messages(source_dirs.messages.as_deref(), &results_dir, &mut stats)?;
    emit_progress(&mut on_progress, step_fraction(3.0), "Analyzing servers...");
    analyze_servers(source_dirs.servers.as_deref(), &mut stats)?;
    emit_progress(
        &mut on_progress,
        step_fraction(4.0),
        "Analyzing support tickets...",
    );
    analyze_support_tickets(source_dirs.support_tickets.as_deref(), &mut stats)?;
    emit_progress(
        &mut on_progress,
        step_fraction(5.0),
        "Analyzing activity events...",
    );
    analyze_activity(
        source_dirs.activity.as_deref(),
        &mut stats,
        |activity_fraction, detail| {
            let activity_fraction = activity_fraction.clamp(0.0, 1.0);
            let overall_fraction = step_fraction(5.0 + activity_fraction);
            emit_progress(
                &mut on_progress,
                overall_fraction,
                format!("Analyzing activity events... {detail}"),
            );
        },
    )?;
    emit_progress(
        &mut on_progress,
        step_fraction(6.0),
        "Analyzing activities...",
    );
    analyze_activities(source_dirs.activities.as_deref(), &mut stats)?;
    emit_progress(
        &mut on_progress,
        step_fraction(7.0),
        "Analyzing programs...",
    );
    analyze_programs(source_dirs.programs.as_deref(), &mut stats)?;

    emit_progress(&mut on_progress, step_fraction(8.0), "Writing results...");
    let data_path = results_dir.join("data.json");
    fs::write(
        &data_path,
        serde_json::to_string_pretty(&stats)
            .with_context(|| "failed to serialize data.json".to_owned())?,
    )
    .with_context(|| format!("failed to write {}", data_path.display()))?;

    emit_progress(&mut on_progress, 1.0, "Analysis complete.");
    Ok(stats)
}

fn emit_progress<F, S>(on_progress: &mut F, fraction: f32, label: S)
where
    F: FnMut(AnalysisProgress),
    S: Into<String>,
{
    on_progress(AnalysisProgress {
        fraction,
        label: label.into(),
    });
}

pub fn read_data(results_dir: &Path) -> Result<Option<AnalysisData>> {
    let data_path = results_dir.join("data.json");
    if !data_path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(&data_path)
        .with_context(|| format!("failed to read {}", data_path.display()))?;
    let parsed: AnalysisData =
        serde_json::from_str(&data).with_context(|| "failed to parse data.json".to_owned())?;
    Ok(Some(parsed))
}

// ── Source dir discovery ──────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct SourceDirs {
    account: Option<PathBuf>,
    activity: Option<PathBuf>,
    activities: Option<PathBuf>,
    messages: Option<PathBuf>,
    programs: Option<PathBuf>,
    servers: Option<PathBuf>,
    support_tickets: Option<PathBuf>,
}

impl SourceDirs {
    fn discover(package_dir: &Path, aliases: &SourceAliases) -> Result<Self> {
        Ok(Self {
            account: resolve_optional_subdir(package_dir, &aliases.account)?,
            activity: resolve_optional_subdir(package_dir, &aliases.activity)?,
            activities: resolve_optional_subdir(package_dir, &aliases.activities)?,
            messages: resolve_optional_subdir(package_dir, &aliases.messages)?,
            programs: resolve_optional_subdir(package_dir, &aliases.programs)?,
            servers: resolve_optional_subdir(package_dir, &aliases.servers)?,
            support_tickets: resolve_optional_subdir(package_dir, &aliases.support_tickets)?,
        })
    }

    fn presence_map(&self) -> BTreeMap<String, bool> {
        let mut map = BTreeMap::new();
        map.insert("account".to_owned(), self.account.is_some());
        map.insert("activity".to_owned(), self.activity.is_some());
        map.insert("activities".to_owned(), self.activities.is_some());
        map.insert("messages".to_owned(), self.messages.is_some());
        map.insert("programs".to_owned(), self.programs.is_some());
        map.insert("servers".to_owned(), self.servers.is_some());
        map.insert("support_tickets".to_owned(), self.support_tickets.is_some());
        map
    }
}

// ── Analysis functions ────────────────────────────────────────────────────────

fn analyze_account(account_dir: &Path, stats: &mut AnalysisData) -> Result<()> {
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

fn analyze_servers(servers_dir: Option<&Path>, stats: &mut AnalysisData) -> Result<()> {
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

fn analyze_support_tickets(support_dir: Option<&Path>, stats: &mut AnalysisData) -> Result<()> {
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

#[derive(Debug, Deserialize)]
struct ActivityEventLine<'a> {
    #[serde(borrow, default)]
    event_type: Option<Cow<'a, str>>,
}

#[derive(Debug)]
struct ActivityFileTask {
    index: usize,
    path: PathBuf,
    size: u64,
    short_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActivityFileStats {
    pub event_lines: u64,
    pub parse_errors: u64,
    pub event_types: BTreeMap<String, u64>,
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

fn analyze_activity<F>(
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
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| {
            path.extension()
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
    let mut tasks_paths = Vec::new(); // keep track of paths for cache update

    for path in files {
        let mtime = get_mtime_ms(&path);
        let rel_path = path.strip_prefix(activity_dir)
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
        let short_name = path.file_name()
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
        on_progress(1.0, "All activity logs up to date (cached)".to_owned());
        return Ok(());
    }

    stats.activity.files += tasks.len() as u64;

    let total_tasks = tasks.len();
    let file_sizes: Vec<u64> = tasks.iter().map(|t| t.size).collect();
    let file_names: Vec<String> = tasks.iter().map(|t| t.short_name.clone()).collect();
    let total_bytes: u64 = file_sizes.iter().sum::<u64>().max(1);
    let mut file_bytes_read = vec![0_u64; total_tasks];
    let mut total_bytes_read = 0_u64;

    let worker_count = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(total_tasks)
        .max(1);

    let task_queue = Arc::new(Mutex::new(VecDeque::from(tasks)));
    let (tx, rx) = mpsc::channel::<ActivityWorkerEvent>();
    let mut worker_handles = Vec::with_capacity(worker_count);

    for _ in 0..worker_count {
        let queue = Arc::clone(&task_queue);
        let worker_tx = tx.clone();
        worker_handles.push(thread::spawn(move || {
            loop {
                let task = {
                    let mut guard = match queue.lock() {
                        Ok(guard) => guard,
                        Err(_) => return,
                    };
                    guard.pop_front()
                };
                let Some(task) = task else { break; };
                match process_activity_file(&task, &worker_tx) {
                    Ok(file_stats) => {
                        let _ = worker_tx.send(ActivityWorkerEvent::Finished {
                            file_index: task.index,
                            stats: file_stats,
                        });
                    }
                    Err(err) => {
                        let _ = worker_tx.send(ActivityWorkerEvent::Failed {
                            _file_index: task.index,
                            error: format!("{}: {err}", task.path.display()),
                        });
                    }
                }
            }
        }));
    }
    drop(tx);

    let mut finished_files = 0usize;
    let mut first_error: Option<anyhow::Error> = None;

    while finished_files < total_tasks {
        let event = match rx.recv() {
            Ok(event) => event,
            Err(_) => break,
        };

        match event {
            ActivityWorkerEvent::Progress { file_index, bytes_read } => {
                let capped = bytes_read.min(file_sizes[file_index]);
                if capped > file_bytes_read[file_index] {
                    total_bytes_read = total_bytes_read.saturating_add(capped - file_bytes_read[file_index]);
                    file_bytes_read[file_index] = capped;
                }
                let fraction = (total_bytes_read as f32 / total_bytes as f32).clamp(0.0, 1.0);
                on_progress(fraction, format!("file {}/{}: {}", file_index + 1, total_tasks, file_names[file_index]));
            }
            ActivityWorkerEvent::Finished { file_index, stats: file_stats } => {
                if file_sizes[file_index] > file_bytes_read[file_index] {
                    total_bytes_read = total_bytes_read.saturating_add(file_sizes[file_index] - file_bytes_read[file_index]);
                    file_bytes_read[file_index] = file_sizes[file_index];
                }

                stats.activity.total_events += file_stats.event_lines;
                stats.activity.parse_errors += file_stats.parse_errors;
                for (et, c) in &file_stats.event_types {
                    increment_counter(&mut stats.activity.by_event_type, et, *c);
                }

                // Update cache
                if let Some((_, rel_path, mtime)) = tasks_paths.iter().find(|(idx, _, _)| *idx == file_index) {
                    next_activity_cache.insert(rel_path.clone(), ActivityFileCache {
                        mtime_ms: *mtime,
                        stats: file_stats,
                    });
                }

                finished_files += 1;
                let fraction = (total_bytes_read as f32 / total_bytes as f32).clamp(0.0, 1.0);
                on_progress(fraction, format!("file {}/{}: {} complete", file_index + 1, total_tasks, file_names[file_index]));
            }
            ActivityWorkerEvent::Failed { _file_index: _, error } => {
                finished_files += 1;
                if first_error.is_none() { first_error = Some(anyhow!(error)); }
            }
        }
    }

    for h in worker_handles { let _ = h.join(); }
    if let Some(err) = first_error { return Err(err); }

    stats.activity_cache = next_activity_cache;
    Ok(())
}

fn process_activity_file(
    task: &ActivityFileTask,
    tx: &Sender<ActivityWorkerEvent>,
) -> Result<ActivityFileStats> {
    const REPORT_BYTE_INTERVAL: u64 = 8 * 1024 * 1024;
    const READ_BUFFER_CAPACITY: usize = 1024 * 1024;

    let file = File::open(&task.path)
        .with_context(|| format!("failed to open {}", task.path.display()))?;
    let mut reader = BufReader::with_capacity(READ_BUFFER_CAPACITY, file);
    let mut line = Vec::with_capacity(16 * 1024);
    let mut bytes_read = 0_u64;
    let mut next_report_bytes = REPORT_BYTE_INTERVAL;
    let mut stats = ActivityFileStats::default();

    loop {
        line.clear();
        let read = reader.read_until(b'\n', &mut line)?;
        if read == 0 {
            break;
        }

        bytes_read = bytes_read.saturating_add(read as u64);
        if bytes_read >= next_report_bytes {
            let _ = tx.send(ActivityWorkerEvent::Progress {
                file_index: task.index,
                bytes_read,
            });
            next_report_bytes = next_report_bytes.saturating_add(REPORT_BYTE_INTERVAL);
        }

        let line = trim_ascii_whitespace(&line);
        if line.is_empty() {
            continue;
        }

        stats.event_lines += 1;
        match serde_json::from_slice::<ActivityEventLine>(line) {
            Ok(value) => {
                if let Some(event_type) = value.event_type {
                    increment_counter(&mut stats.event_types, event_type, 1);
                } else {
                    increment_counter(&mut stats.event_types, "unknown", 1);
                }
            }
            Err(_) => {
                stats.parse_errors += 1;
            }
        }
    }

    let _ = tx.send(ActivityWorkerEvent::Progress {
        file_index: task.index,
        bytes_read: task.size,
    });

    Ok(stats)
}

fn analyze_activities(activities_dir: Option<&Path>, stats: &mut AnalysisData) -> Result<()> {
    let Some(activities_dir) = activities_dir else {
        return Ok(());
    };

    for entry in WalkDir::new(activities_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        stats.activities.files += 1;

        let file_name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if file_name == "favorite_games.json" {
            if let Ok(value) = read_json_value(entry.path()) {
                stats.activities.favorite_games = value
                    .get("favorite_games")
                    .and_then(|v| match v {
                        Value::Number(n) => n.as_u64(),
                        _ => None,
                    })
                    .or(stats.activities.favorite_games);
            }
        } else if file_name == "preferences.json" {
            if let Ok(value) = read_json_value(entry.path()) {
                if let Value::Array(items) = value {
                    stats.activities.preferences_entries += items.len() as u64;
                }
            }
        } else if file_name == "user_data.json" {
            if let Ok(value) = read_json_value(entry.path()) {
                if let Value::Object(map) = value {
                    stats.activities.user_data_apps += map.len() as u64;
                }
            }
        }
    }
    Ok(())
}

fn analyze_programs(programs_dir: Option<&Path>, stats: &mut AnalysisData) -> Result<()> {
    let Some(programs_dir) = programs_dir else {
        return Ok(());
    };
    stats.programs.files = WalkDir::new(programs_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .count() as u64;
    Ok(())
}

fn summarize_ticket(value: &Value, stats: &mut AnalysisData) {
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
        increment_counter(&mut stats.support_tickets.by_month, created_month.clone(), 1);
        increment_counter(&mut stats.support_tickets.activity_by_month, created_month, 1);
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

// ── Message analysis ──────────────────────────────────────────────────────────

struct ChannelTask {
    id: String,
    messages_path: PathBuf,
    channel_path: PathBuf,
    mtime_messages: u64,
    mtime_channel: u64,
}

enum ChannelWorkerEvent {
    Finished { id: String, stats: ChannelAnalysisCache },
    Failed { id: String, error: String },
}

fn analyze_messages(
    messages_dir: Option<&Path>,
    _results_dir: &Path,
    stats: &mut AnalysisData,
) -> Result<()> {
    let Some(messages_dir) = messages_dir else {
        stats.warnings.push("Messages directory missing; message analysis skipped.".to_owned());
        return Ok(());
    };

    let custom_emoji_re = Regex::new(r"<a?:[A-Za-z0-9_]+:\d+>")?;
    let hour_re = Regex::new(r"(?:T| )(\d{2}):(\d{2}):(\d{2})")?;
    let date_re = Regex::new(r"^(\d{4})-(\d{2})-(\d{2})")?;
    let word_re = Regex::new(r"(?i)\b[a-z]{3,15}\b")?;

    let mut channel_dirs: Vec<PathBuf> = fs::read_dir(messages_dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    channel_dirs.sort();

    let mut new_channels_cache = BTreeMap::new();
    let mut tasks = Vec::new();
    let mut total_word_freq: HashMap<String, u64> = HashMap::new();
    let mut total_char_freq: HashMap<char, u64> = HashMap::new();
    let mut channel_counts: Vec<(String, u64)> = Vec::new();

    for channel_dir in channel_dirs {
        let channel_id = channel_dir.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_owned();

        let messages_path = channel_dir.join("messages.json");
        if !messages_path.is_file() {
            continue;
        }

        let channel_path = channel_dir.join("channel.json");
        let mtime_msg = get_mtime_ms(&messages_path);
        let mtime_ch = get_mtime_ms(&channel_path);

        if let Some(cached) = stats.channels_cache.get(&channel_id) {
            if cached.mtime_messages == mtime_msg && cached.mtime_channel == mtime_ch {
                // Merge into global stats immediately
                stats.messages.channels += 1;
                stats.messages.total += cached.message_count;
                stats.messages.with_content += cached.messages_with_content;
                stats.messages.with_attachments += cached.attachment_count;
                stats.messages.attachment_links.extend(cached.attachment_links.clone());
                increment_counter(&mut stats.messages.by_channel_type, &cached.channel_type, 1);
                stats.messages.temporal.merge(&cached.temporal);
                stats.messages.content.merge(&cached.content);

                for (w, c) in &cached.word_frequency {
                    *total_word_freq.entry(w.clone()).or_insert(0) += c;
                }
                for (ch, c) in &cached.content.character_frequency {
                    if let Some(first_char) = ch.chars().next() {
                        *total_char_freq.entry(first_char).or_insert(0) += c;
                    }
                }

                channel_counts.push((cached.channel_title.clone(), cached.message_count));
                new_channels_cache.insert(channel_id, cached.clone());
                continue;
            }
        }

        tasks.push(ChannelTask {
            id: channel_id,
            messages_path,
            channel_path,
            mtime_messages: mtime_msg,
            mtime_channel: mtime_ch,
        });
    }

    if !tasks.is_empty() {
        let worker_count = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .min(tasks.len())
            .max(1);

        let task_queue = Arc::new(Mutex::new(VecDeque::from(tasks)));
        let (tx, rx) = mpsc::channel::<ChannelWorkerEvent>();

        for _ in 0..worker_count {
            let queue = Arc::clone(&task_queue);
            let worker_tx = tx.clone();
            let c_re = custom_emoji_re.clone();
            let h_re = hour_re.clone();
            let d_re = date_re.clone();
            let w_re = word_re.clone();

            thread::spawn(move || {
                loop {
                    let task = {
                        let mut guard = match queue.lock() {
                            Ok(g) => g,
                            Err(_) => return,
                        };
                        guard.pop_front()
                    };
                    let Some(task) = task else { break; };

                    let mut ch_stats = ChannelAnalysisCache {
                        mtime_messages: task.mtime_messages,
                        mtime_channel: task.mtime_channel,
                        ..Default::default()
                    };

                    if task.channel_path.is_file() {
                        if let Ok(val) = read_json_value(&task.channel_path) {
                            ch_stats.channel_type = val.get("type")
                                .and_then(value_to_plain_string)
                                .unwrap_or_else(|| "unknown".to_owned());
                            ch_stats.channel_title = channel_title(Some(&val), &task.id);
                        }
                    } else {
                        ch_stats.channel_title = task.id.clone();
                    }

                    match read_records_json_or_ndjson(&task.messages_path) {
                        Ok(records) => {
                            for record in records {
                                ch_stats.message_count += 1;
                                let content = extract_message_content(&record);
                                let attachments = extract_attachment_urls(&record);

                                if let Some(ts) = pick_str(&record, &["Timestamp", "timestamp", "timestamp_ms", "date"]) {
                                    if let Some(caps) = h_re.captures(ts) {
                                        if let Ok(hr) = caps[1].parse::<u32>() {
                                            *ch_stats.temporal.by_hour.entry(hr).or_insert(0) += 1;
                                        }
                                    }
                                    if let Some(caps) = d_re.captures(ts) {
                                        let y: u32 = caps[1].parse().unwrap_or(0);
                                        let m: u32 = caps[2].parse().unwrap_or(0);
                                        let d: u32 = caps[3].parse().unwrap_or(0);
                                        if (1..=12).contains(&m) {
                                            *ch_stats.temporal.by_month.entry(m).or_insert(0) += 1;
                                        }
                                        if y >= 1 && m >= 1 && d >= 1 {
                                            let dow = day_of_week(y, m, d);
                                            *ch_stats.temporal.by_day_of_week.entry(dow).or_insert(0) += 1;
                                        }
                                        let ds = format!("{y:04}-{m:02}-{d:02}");
                                        if ch_stats.temporal.first_message_date.as_deref().is_none_or(|f| ds < f.to_owned()) {
                                            ch_stats.temporal.first_message_date = Some(ds.clone());
                                        }
                                        if ch_stats.temporal.last_message_date.as_deref().is_none_or(|l| ds > l.to_owned()) {
                                            ch_stats.temporal.last_message_date = Some(ds);
                                        }
                                    }
                                }

                                if !content.is_empty() {
                                    ch_stats.messages_with_content += 1;
                                    ch_stats.content.total_chars += content.chars().count() as u64;
                                    ch_stats.content.linebreaks += content.matches('\n').count() as u64;
                                    ch_stats.content.emoji_custom += c_re.find_iter(&content).count() as u64;
                                    for g in content.graphemes(true) {
                                        if emojis::get(g).is_some() { ch_stats.content.emoji_unicode += 1; }
                                    }
                                    for ch in content.chars() {
                                        *ch_stats.content.character_frequency.entry(ch.to_string()).or_insert(0) += 1;
                                    }
                                    for mat in w_re.find_iter(&content.to_ascii_lowercase()) {
                                        let w = mat.as_str();
                                        if !is_stop_word(w) {
                                            *ch_stats.word_frequency.entry(w.to_owned()).or_insert(0) += 1;
                                        }
                                    }
                                }

                                if !attachments.is_empty() {
                                    ch_stats.attachment_count += attachments.len() as u64;
                                    for url in attachments {
                                        if url.starts_with("https://cdn.discordapp.com/attachments/") {
                                            ch_stats.attachment_links.push(url);
                                        }
                                    }
                                }
                            }
                            let _ = worker_tx.send(ChannelWorkerEvent::Finished { id: task.id, stats: ch_stats });
                        }
                        Err(e) => {
                            let _ = worker_tx.send(ChannelWorkerEvent::Failed { id: task.id, error: e.to_string() });
                        }
                    }
                }
            });
        }
        drop(tx);

        while let Ok(event) = rx.recv() {
            match event {
                ChannelWorkerEvent::Finished { id, stats: cache_entry } => {
                    stats.messages.channels += 1;
                    stats.messages.total += cache_entry.message_count;
                    stats.messages.with_content += cache_entry.messages_with_content;
                    stats.messages.with_attachments += cache_entry.attachment_count;
                    stats.messages.attachment_links.extend(cache_entry.attachment_links.clone());
                    increment_counter(&mut stats.messages.by_channel_type, &cache_entry.channel_type, 1);
                    stats.messages.temporal.merge(&cache_entry.temporal);
                    stats.messages.content.merge(&cache_entry.content);

                    for (w, c) in &cache_entry.word_frequency {
                        *total_word_freq.entry(w.clone()).or_insert(0) += c;
                    }
                    for (ch, c) in &cache_entry.content.character_frequency {
                        if let Some(first_char) = ch.chars().next() {
                            *total_char_freq.entry(first_char).or_insert(0) += c;
                        }
                    }

                    channel_counts.push((cache_entry.channel_title.clone(), cache_entry.message_count));
                    new_channels_cache.insert(id, cache_entry);
                }
                ChannelWorkerEvent::Failed { id, error } => {
                    stats.warnings.push(format!("Channel {id} failed: {error}"));
                }
            }
        }
    }
    stats.channels_cache = new_channels_cache;

    // Finalize global aggregate stats
    stats.messages.content.distinct_characters = total_char_freq.len();
    stats.messages.content.avg_length_chars = if stats.messages.with_content > 0 {
        (stats.messages.content.total_chars as f64) / (stats.messages.with_content as f64)
    } else { 0.0 };

    let mut words_vec: Vec<_> = total_word_freq.into_iter().collect();
    words_vec.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    words_vec.truncate(100);
    stats.messages.content.top_words = words_vec;

    channel_counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    channel_counts.truncate(25);
    stats.messages.top_channels = channel_counts;

    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Tomohiko Sakamoto's day-of-week algorithm. Returns 0=Monday … 6=Sunday.
fn day_of_week(year: u32, month: u32, day: u32) -> u32 {
    const T: [u32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    let dow = (y + y / 4 - y / 100 + y / 400 + T[(month - 1) as usize] + day) % 7;
    // Sakamoto gives 0=Sunday; convert to 0=Monday
    if dow == 0 { 6 } else { dow - 1 }
}

fn utc_now_iso8601() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400; // days since 1970-01-01
    // Simple date reconstruction (no leap-second handling)
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let leap = is_leap(year);
        let days_in_year = if leap { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let leap = is_leap(year);
    let month_days: [u64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u64;
    for md in &month_days {
        if days < *md {
            break;
        }
        days -= md;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(y: u64) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
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

fn increment_counter(map: &mut BTreeMap<String, u64>, key: impl Into<String>, by: u64) {
    let entry = map.entry(key.into()).or_insert(0);
    *entry += by;
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
fn get_mtime_ms(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
