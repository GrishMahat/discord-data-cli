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
    /// Derived insight metrics (billing, privacy, social, voice, devices...).
    #[serde(default)]
    pub insights: Insights,
    /// Aggregated activity-event insights; invalidated when any input changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insights_cache: Option<InsightsCache>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelAnalysisCache {
    /// Bump when per-channel extraction changes; older caches get recomputed.
    pub cache_version: u32,
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
    pub link_messages: u64,
    pub total_links: u64,
    pub question_messages: u64,
    pub link_domains: BTreeMap<String, u64>,
    /// Longest single message in this channel (chars).
    pub max_message_chars: u64,
    /// Timestamp (date part) of that longest message.
    pub max_message_date: Option<String>,
}

/// Version of the per-channel extraction schema. Bump to force recompute.
pub const CHANNEL_CACHE_VERSION: u32 = 4;

/// Version of the EventAggregate shape. Bump to invalidate cached aggregates.
pub const EVENT_AGGREGATE_VERSION: u64 = 2;

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
    pub fn merge(&mut self, other: &Self) {
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
    /// date ("2024-03-05") -> message count (drives the activity heatmap)
    pub by_day: BTreeMap<String, u64>,
}

impl Temporal {
    pub fn merge(&mut self, other: &Self) {
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
        for (d, c) in &other.by_day {
            *self.by_day.entry(d.clone()).or_insert(0) += c;
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

// ---------------------------------------------------------------------------
// Insights: derived, human-meaningful metrics beyond raw counts.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Insights {
    pub billing: BillingInsights,
    pub privacy: PrivacyInsights,
    pub social: SocialInsights,
    pub links: LinkInsights,
    pub voice: VoiceInsights,
    pub devices: DeviceInsights,
    pub sessions: SessionInsights,
    pub records: RecordsInsights,
    /// Servers ranked by engagement (telemetry events), best first.
    pub servers_engaged: Vec<ServerEngagement>,
}

/// One payment in the billing timeline.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PaymentEntry {
    pub date: String,
    pub description: String,
    pub amount: i64,
    pub currency: String,
    /// Gateway code (1=Stripe, 3=Apple, 4=Google, 8=Virtual Currency...).
    pub gateway: u32,
    pub refunded: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BillingInsights {
    pub payments_total: u64,
    /// currency -> total amount spent
    pub totals_by_currency: BTreeMap<String, i64>,
    /// gateway code -> payment count
    pub by_gateway: BTreeMap<u32, u64>,
    /// Newest first.
    pub timeline: Vec<PaymentEntry>,
    pub entitlements_count: u64,
    pub coin_transactions: u64,
}

/// Privacy posture. Presence flags only — no emails, phones or IPs stored.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrivacyInsights {
    pub email_verified: bool,
    pub mfa_enabled: bool,
    pub has_phone: bool,
    pub has_payment_source: bool,
    pub payment_sources: u64,
    pub data_access_requests: u64,
    pub account_flags: Vec<String>,
    pub premium_until: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SocialInsights {
    pub dm_channels: u64,
    pub group_dm_channels: u64,
    pub dm_messages: u64,
    pub group_dm_messages: u64,
    /// DM contacts, best first.
    pub top_contacts: Vec<ContactStat>,
    /// Group DMs, best first.
    pub top_groups: Vec<ContactStat>,
}

/// One DM contact or group DM with activity summary.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContactStat {
    pub name: String,
    pub messages: u64,
    pub first_date: Option<String>,
    pub last_date: Option<String>,
}

/// A server ranked by telemetry engagement.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerEngagement {
    pub name: String,
    /// Events observed in this server (channel opens, messages sent, ...).
    pub events: u64,
    pub audit_log_entries: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LinkInsights {
    pub messages_with_links: u64,
    pub total_links: u64,
    pub question_messages: u64,
    /// domain -> link count, best first (rendered).
    pub top_domains: Vec<(String, u64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VoiceInsights {
    pub connections: u64,
    pub disconnects: u64,
    pub reconnects: u64,
    pub media_sessions: u64,
    pub speaking_starts: u64,
    pub listening_starts: u64,
    pub avg_connect_ms: f64,
    pub avg_ping_ms: f64,
    pub avg_mos: f64,
    pub packets_sent: u64,
    pub packets_lost: u64,
    /// Total time connected across all sessions, minutes.
    pub connected_minutes: f64,
    pub speaking_minutes: f64,
    pub listening_minutes: f64,
    /// connection type (wifi/ethernet/cellular...) -> minutes
    pub minutes_by_connection_type: BTreeMap<String, f64>,
    pub top_media_hosts: Vec<(String, u64)>,
    pub disconnect_reasons: Vec<(String, u64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceInsights {
    pub by_os: BTreeMap<String, u64>,
    /// "Discord Client", browser names...
    pub by_client: BTreeMap<String, u64>,
    pub by_release_channel: BTreeMap<String, u64>,
    /// client build number -> event count
    pub by_client_build: BTreeMap<String, u64>,
    pub by_isp: BTreeMap<String, u64>,
    pub by_country: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionInsights {
    pub active_days: u64,
    pub first_active_day: Option<String>,
    pub last_active_day: Option<String>,
    pub longest_daily_streak: u64,
    pub session_starts: u64,
    /// Single busiest telemetry day: (date, event count).
    pub busiest_day: Option<(String, u64)>,
    /// month ("2024-03") -> event count
    pub events_by_month: BTreeMap<String, u64>,
    pub top_event_types: Vec<(String, u64)>,
}

/// Fun personal records surfaced on the Insights screen.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecordsInsights {
    pub first_message_date: Option<String>,
    pub last_message_date: Option<String>,
    pub longest_message_chars: u64,
    pub longest_message_date: Option<String>,
    /// (channel title, message count)
    pub biggest_channel: Option<(String, u64)>,
}

/// Aggregated insight counters over all activity event files. Cached behind a
/// signature of the input files' mtimes/sizes.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InsightsCache {
    pub signature: u64,
    pub aggregate: EventAggregate,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventAggregate {
    pub events_total: u64,
    pub duplicates_skipped: u64,
    pub parse_errors: u64,
    pub days: BTreeMap<String, u64>,
    pub by_month: BTreeMap<String, u64>,
    /// event_type -> count, capped at 256 distinct types.
    pub by_event_type: BTreeMap<String, u64>,
    /// guild_id -> event count, capped at 128 guilds.
    pub guild_events: BTreeMap<String, u64>,
    pub session_starts: u64,
    // voice
    pub voice_connections: u64,
    pub voice_disconnects: u64,
    pub reconnects: u64,
    pub media_sessions: u64,
    pub speaking_starts: u64,
    pub listening_starts: u64,
    pub connect_time_ms_sum: f64,
    pub connect_time_n: u64,
    pub ping_sum: f64,
    pub ping_n: u64,
    pub mos_sum: f64,
    pub mos_n: u64,
    pub packets_sent: u64,
    pub packets_lost: u64,
    pub connected_ms: f64,
    pub speaking_ms: f64,
    pub listening_ms: f64,
    pub connection_type_ms: BTreeMap<String, f64>,
    pub media_hosts: BTreeMap<String, u64>,
    pub disconnect_reasons: BTreeMap<String, u64>,
    // devices
    pub by_os: BTreeMap<String, u64>,
    pub by_client: BTreeMap<String, u64>,
    pub by_release_channel: BTreeMap<String, u64>,
    pub by_client_build: BTreeMap<String, u64>,
    pub by_isp: BTreeMap<String, u64>,
    pub by_country: BTreeMap<String, u64>,
}

impl EventAggregate {
    pub fn merge(&mut self, other: &Self) {
        self.events_total += other.events_total;
        self.duplicates_skipped += other.duplicates_skipped;
        self.parse_errors += other.parse_errors;
        merge_map(&mut self.days, &other.days);
        merge_map(&mut self.by_month, &other.by_month);
        merge_map(&mut self.by_event_type, &other.by_event_type);
        merge_map(&mut self.guild_events, &other.guild_events);
        self.session_starts += other.session_starts;
        self.voice_connections += other.voice_connections;
        self.voice_disconnects += other.voice_disconnects;
        self.reconnects += other.reconnects;
        self.media_sessions += other.media_sessions;
        self.speaking_starts += other.speaking_starts;
        self.listening_starts += other.listening_starts;
        self.connect_time_ms_sum += other.connect_time_ms_sum;
        self.connect_time_n += other.connect_time_n;
        self.ping_sum += other.ping_sum;
        self.ping_n += other.ping_n;
        self.mos_sum += other.mos_sum;
        self.mos_n += other.mos_n;
        self.packets_sent += other.packets_sent;
        self.packets_lost += other.packets_lost;
        self.connected_ms += other.connected_ms;
        self.speaking_ms += other.speaking_ms;
        self.listening_ms += other.listening_ms;
        merge_f64(&mut self.connection_type_ms, &other.connection_type_ms);
        merge_map(&mut self.media_hosts, &other.media_hosts);
        merge_map(&mut self.disconnect_reasons, &other.disconnect_reasons);
        merge_map(&mut self.by_os, &other.by_os);
        merge_map(&mut self.by_client, &other.by_client);
        merge_map(&mut self.by_release_channel, &other.by_release_channel);
        merge_map(&mut self.by_client_build, &other.by_client_build);
        merge_map(&mut self.by_isp, &other.by_isp);
        merge_map(&mut self.by_country, &other.by_country);
    }
}

fn merge_map(dst: &mut BTreeMap<String, u64>, src: &BTreeMap<String, u64>) {
    for (k, v) in src {
        *dst.entry(k.clone()).or_insert(0) += v;
    }
}

fn merge_f64(dst: &mut BTreeMap<String, f64>, src: &BTreeMap<String, f64>) {
    for (k, v) in src {
        *dst.entry(k.clone()).or_insert(0.0) += v;
    }
}
