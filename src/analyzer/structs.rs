use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AnalysisData {
    pub meta: Meta,
    pub account: Account,
    pub folder_presence: BTreeMap<String, bool>,
    pub warnings: Vec<String>,
    pub messages: Messages,
    pub servers: Servers,
    pub support_tickets: SupportTickets,
    pub activity: Activity,
    pub activities: Activities,
    pub programs: Programs,

    #[serde(skip)]
    pub package_directory: String,
    #[serde(skip)]
    pub results_directory: String,

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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Meta {
    pub tool_version: String,
    pub analyzed_at: String,
    pub package_directory: String,
    pub results_directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Account {
    pub user_id: Option<String>,
    pub username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Messages {
    pub total: u64,
    pub channels: u64,
    pub by_channel_type: BTreeMap<String, u64>,
    pub with_content: u64,
    pub with_attachments: u64,
    pub attachment_links: Vec<String>,
    pub content: ContentStats,
    pub temporal: Temporal,
    pub top_channels: Vec<(String, u64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContentStats {
    pub distinct_characters: usize,
    pub character_frequency: BTreeMap<char, u64>,
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
            *self.character_frequency.entry(*ch).or_insert(0) += count;
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Servers {
    pub count: u64,
    pub index_entries: u64,
    pub audit_log_entries: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SupportTickets {
    pub count: u64,
    pub comments: u64,
    pub tickets_with_comments: u64,
    pub avg_comments_per_ticket: f64,
    pub by_status: BTreeMap<String, u64>,
    pub by_priority: BTreeMap<String, u64>,
    pub by_month: BTreeMap<String, u64>,
    pub activity_by_month: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Activity {
    pub files: u64,
    pub total_events: u64,
    pub parse_errors: u64,
    pub by_event_type: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Activities {
    pub files: u64,
    pub preferences_entries: u64,
    pub user_data_apps: u64,
    pub favorite_games: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Programs {
    pub files: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActivityFileStats {
    pub event_lines: u64,
    pub parse_errors: u64,
    pub event_types: BTreeMap<String, u64>,
}
