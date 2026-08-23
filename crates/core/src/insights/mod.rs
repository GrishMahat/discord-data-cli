//! Phase 2 "Data Insight Expansion": turn raw export folders into meaningful,
//! human-readable analytics — billing timeline, privacy posture, DM contacts,
//! link intelligence, voice/RTC quality, device profile and sessionization.
//!
//! The multi-GB activity logs are streamed in one deduplicated pass and cached
//! behind a signature of the input files (`AnalysisData::insights_cache`), so
//! unchanged exports skip the scan entirely on later analyses.

use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::analyzer::structs::{
    BillingInsights, ContactStat, EVENT_AGGREGATE_VERSION, EventAggregate, Insights, PaymentEntry,
    PrivacyInsights, ServerEngagement, SocialInsights,
};
use crate::data::utils::{pick_str, read_json_value, value_to_plain_string};

/// Billing + privacy posture from the Account folder.
pub fn compute_account(
    account_dir: Option<&Path>,
    stats: &mut crate::analyzer::AnalysisData,
) -> Result<()> {
    compute_billing_and_privacy(account_dir, stats)
}

/// DM / group-DM social summary from the Messages folders.
pub fn compute_social(
    messages_dir: Option<&Path>,
    stats: &mut crate::analyzer::AnalysisData,
) -> Result<()> {
    compute_social_inner(messages_dir, stats)
}

// ---------------------------------------------------------------------------
// Billing + privacy posture
// ---------------------------------------------------------------------------

fn compute_billing_and_privacy(
    account_dir: Option<&Path>,
    stats: &mut crate::analyzer::AnalysisData,
) -> Result<()> {
    let Some(account_dir) = account_dir else {
        return Ok(());
    };
    let exports_dir = account_dir.join("user_data_exports");

    // ---- user.json -> privacy flags (presence only, never raw contact info)
    let mut privacy = PrivacyInsights::default();
    if let Ok(user) = read_json_value(&account_dir.join("user.json")) {
        privacy.email_verified = user
            .get("verified")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        privacy.has_phone = user
            .get("phone")
            .and_then(value_to_plain_string)
            .is_some_and(|p| !p.is_empty());
        privacy.premium_until = user
            .get("premium_until")
            .and_then(value_to_plain_string)
            .filter(|v| v != "null" && !v.is_empty());
        if let Some(Value::Array(flags)) = user.get("flags") {
            for f in flags {
                if let Some(s) = f.as_str() {
                    privacy.account_flags.push(s.to_owned());
                    if s.starts_with("MFA_") {
                        privacy.mfa_enabled = true;
                    }
                }
            }
        }
    }

    let record_count = |path: &Path| -> u64 {
        read_json_value(path)
            .ok()
            .and_then(|v| {
                v.get("record_count").and_then(Value::as_u64).or_else(|| {
                    v.get("records")
                        .and_then(Value::as_array)
                        .map(|a| a.len() as u64)
                })
            })
            .unwrap_or(0)
    };

    privacy.payment_sources =
        record_count(&exports_dir.join("discord_billing/payment_sources.json"));
    privacy.has_payment_source = privacy.payment_sources > 0;
    privacy.data_access_requests =
        record_count(&exports_dir.join("discord_harvests/data_subject_access_requests.json"));
    stats.insights.privacy = privacy;

    // ---- billing
    let mut billing = BillingInsights::default();
    let payments_path = exports_dir.join("discord_billing/payments.json");
    if let Ok(payments) = read_json_value(&payments_path) {
        let mut entries: Vec<PaymentEntry> = Vec::new();
        if let Some(records) = payments.get("records").and_then(Value::as_array) {
            for rec in records {
                let amount = num_i64(rec.get("amount")).unwrap_or(0);
                let currency = str_or(rec.get("currency"), "unknown");
                let date = normalize_iso(str_or(rec.get("created_at"), ""));
                entries.push(PaymentEntry {
                    date,
                    description: str_or(rec.get("description"), "(no description)"),
                    amount,
                    currency,
                    gateway: num_u32(rec.get("payment_gateway")).unwrap_or(0),
                    refunded: num_i64(rec.get("amount_refunded")).unwrap_or(0),
                });
            }
        }
        for p in &entries {
            *billing
                .totals_by_currency
                .entry(p.currency.clone())
                .or_insert(0) += p.amount;
            *billing.by_gateway.entry(p.gateway).or_insert(0) += 1;
        }
        entries.sort_by(|a, b| {
            b.date
                .cmp(&a.date)
                .then_with(|| a.description.cmp(&b.description))
        });
        billing.payments_total = entries.len() as u64;
        billing.timeline = entries;
    }
    billing.entitlements_count =
        record_count(&exports_dir.join("discord_billing/entitlements.json"));
    billing.coin_transactions =
        record_count(&exports_dir.join("discord_virtual_currency/coin_transactions.json"));
    stats.insights.billing = billing;
    Ok(())
}

// ---------------------------------------------------------------------------
// DM / group-DM social summary
// ---------------------------------------------------------------------------

fn compute_social_inner(
    messages_dir: Option<&Path>,
    stats: &mut crate::analyzer::AnalysisData,
) -> Result<()> {
    let Some(root) = resolve_optional_subdir_path(messages_dir) else {
        return Ok(());
    };
    let mut social = SocialInsights::default();

    let Ok(entries) = fs::read_dir(&root) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let dir_name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_owned();
        let Some(cache) = stats.channels_cache.get(&dir_name) else {
            continue;
        };
        match cache.channel_type.to_ascii_uppercase().as_str() {
            "DM" => {
                social.dm_channels += 1;
                social.dm_messages += cache.message_count;
                social.top_contacts.push(ContactStat {
                    name: cache.channel_title.clone(),
                    messages: cache.message_count,
                    first_date: cache.temporal.first_message_date.clone(),
                    last_date: cache.temporal.last_message_date.clone(),
                });
            }
            "GROUP_DM" => {
                social.group_dm_channels += 1;
                social.group_dm_messages += cache.message_count;
                social.top_groups.push(ContactStat {
                    name: cache.channel_title.clone(),
                    messages: cache.message_count,
                    first_date: cache.temporal.first_message_date.clone(),
                    last_date: cache.temporal.last_message_date.clone(),
                });
            }
            _ => {}
        }
    }
    sort_contacts(&mut social.top_contacts);
    sort_contacts(&mut social.top_groups);
    stats.insights.social = social;
    Ok(())
}

fn sort_contacts(v: &mut [ContactStat]) {
    v.sort_by(|a, b| {
        b.messages
            .cmp(&a.messages)
            .then_with(|| a.name.cmp(&b.name))
    });
}

/// Rank servers by telemetry engagement, resolving names from Servers/index.json
/// and counting audit-log entries per server folder.
pub fn rank_servers(
    servers_dir: Option<&Path>,
    stats: &mut crate::analyzer::AnalysisData,
) -> Result<()> {
    if servers_dir.is_none() || stats.insights_cache.is_none() {
        return Ok(());
    }
    let servers_dir = servers_dir.unwrap();
    let index_path = servers_dir.join("index.json");
    let names: BTreeMap<String, String> = read_json_value(&index_path)
        .ok()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();

    let aggregate = &stats.insights_cache.as_ref().unwrap().aggregate;
    let mut ranked: Vec<ServerEngagement> = aggregate
        .guild_events
        .iter()
        .map(|(id, events)| ServerEngagement {
            name: names
                .get(id)
                .cloned()
                .unwrap_or_else(|| format!("server…{}", &id[id.len().saturating_sub(6)..])),
            events: *events,
            audit_log_entries: audit_entry_count(&servers_dir.join(id)),
        })
        .collect();
    ranked.sort_by(|a, b| b.events.cmp(&a.events));
    ranked.truncate(10);
    stats.insights.servers_engaged = ranked;
    Ok(())
}

fn audit_entry_count(server_dir: &Path) -> u64 {
    read_json_value(&server_dir.join("audit-log.json"))
        .ok()
        .and_then(|v| v.as_array().map(|a| a.len() as u64))
        .unwrap_or(0)
}

/// messages_dir is already alias-resolved by the analyzer's SourceDirs.
fn resolve_optional_subdir_path(messages_dir: Option<&Path>) -> Option<PathBuf> {
    messages_dir.map(Path::to_path_buf)
}

/// Mix the aggregate schema version into the cache signature so shape changes
/// (new fields) invalidate previously cached aggregates.
pub fn versioned_signature(files: &[PathBuf]) -> u64 {
    signature_for(files) ^ EVENT_AGGREGATE_VERSION.wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

// ---------------------------------------------------------------------------
// Activity-derived insights: voice quality, devices, sessions
// ---------------------------------------------------------------------------

/// Activity-derived insights: streams (and dedups) the telemetry event logs
/// once, caches the aggregate, and populates `stats.activity` from the same
/// numbers so the rest of the app sees deduplicated counts too.
pub fn compute_activity<F>(
    activity_dir: Option<&Path>,
    stats: &mut crate::analyzer::AnalysisData,
    mut on_progress: F,
) -> Result<()>
where
    F: FnMut(f32, String),
{
    let file_count = count_activity_files(activity_dir);
    compute_activity_insights(activity_dir, stats, &mut on_progress)?;
    if let Some(cache) = &stats.insights_cache {
        let agg = &cache.aggregate;
        let activity = &mut stats.activity;
        activity.files = file_count as u64;
        activity.total_events = agg.events_total;
        activity.parse_errors = agg.parse_errors;
        activity.by_event_type = agg.by_event_type.clone();
    }
    Ok(())
}

fn count_activity_files(activity_dir: Option<&Path>) -> usize {
    let Some(dir) = activity_dir else { return 0 };
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext.to_string_lossy().eq_ignore_ascii_case("json"))
                .unwrap_or(false)
        })
        .count()
}

fn compute_activity_insights<F>(
    activity_dir: Option<&Path>,
    stats: &mut crate::analyzer::AnalysisData,
    on_progress: &mut F,
) -> Result<()>
where
    F: FnMut(f32, String),
{
    let Some(activity_dir) = activity_dir else {
        return Ok(());
    };

    let mut files: Vec<PathBuf> = activity_event_files(activity_dir);
    files.sort();
    if files.is_empty() {
        return Ok(());
    }

    // Signature over every input file. The analytics/modeling/reporting/tns
    // folders contain overlapping copies of the same telemetry events (same
    // event_id), so aggregation MUST be a single deduplicated global pass —
    // per-file caching would double-count across folders.
    let signature = versioned_signature(&files);

    if let Some(cache) = &stats.insights_cache
        && cache.signature == signature
    {
        apply_aggregate(&cache.aggregate, &mut stats.insights);
        on_progress(1.0, "activity insights cached".to_owned());
        return Ok(());
    }

    let total_bytes: u64 = files
        .iter()
        .map(|p| fs::metadata(p).map(|m| m.len()).unwrap_or(0))
        .sum::<u64>()
        .max(1);
    let mut done_bytes: u64 = 0;

    let mut merged = EventAggregate::default();
    // Dedup set: same event can appear in several pipeline folders.
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for path in &files {
        let name = path
            .strip_prefix(activity_dir)
            .unwrap_or(path)
            .display()
            .to_string();
        on_progress(
            done_bytes as f32 / total_bytes as f32,
            format!("Scanning {name}"),
        );
        let agg = stream_file_aggregate(path, &mut seen_ids)?;
        done_bytes += fs::metadata(path).map(|m| m.len()).unwrap_or(1).max(1);
        merged.merge(&agg);
    }

    stats.insights_cache = Some(crate::analyzer::structs::InsightsCache {
        signature,
        aggregate: merged.clone(),
    });
    apply_aggregate(&merged, &mut stats.insights);
    Ok(())
}

fn stream_file_aggregate(
    path: &Path,
    seen_ids: &mut std::collections::HashSet<String>,
) -> Result<EventAggregate> {
    let file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::with_capacity(1024 * 1024, file);
    let mut agg = EventAggregate::default();

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "[" || trimmed == "]" {
            continue;
        }
        let clean = trimmed.strip_suffix(',').unwrap_or(trimmed);
        let Ok(record) = serde_json::from_str::<Value>(clean) else {
            agg.parse_errors += 1;
            continue;
        };
        // Skip events already ingested from another pipeline folder.
        if let Some(id) = pick_str(&record, &["event_id"])
            && !seen_ids.insert(id.to_owned())
        {
            agg.duplicates_skipped += 1;
            continue;
        }
        ingest_record(&record, &mut agg);
    }
    Ok(agg)
}

fn ingest_record(rec: &Value, agg: &mut EventAggregate) {
    agg.events_total += 1;

    let day = event_date(rec);
    if !day.is_empty() {
        *agg.days.entry(day.clone()).or_insert(0) += 1;
        if agg.by_month.len() < 400 && day.len() >= 7 {
            *agg.by_month.entry(day[..7].to_owned()).or_insert(0) += 1;
        }
    }

    let event_type = pick_str(rec, &["event_type"]).unwrap_or("").to_owned();
    if !event_type.is_empty() {
        bump_bounded(&mut agg.by_event_type, &event_type, 256);
    }
    if let Some(guild_id) = pick_str(rec, &["guild_id"]).filter(|s| !s.is_empty()) {
        bump_bounded(&mut agg.guild_events, guild_id, 128);
    }

    match event_type.as_str() {
        "session_start_success" => agg.session_starts += 1,
        "voice_connection_success" => {
            agg.voice_connections += 1;
            if let Some(ms) = num_f64(rec.get("connect_time")) {
                agg.connect_time_ms_sum += ms;
                agg.connect_time_n += 1;
            }
        }
        "voice_disconnect" => {
            agg.voice_disconnects += 1;
            if let Some(r) = rec.get("reconnect").and_then(Value::as_u64)
                && r > 0
            {
                agg.reconnects += 1;
            }
            if let Some(reason) = pick_str(rec, &["reason"]).filter(|s| !s.is_empty()) {
                *agg.disconnect_reasons.entry(reason.to_owned()).or_insert(0) += 1;
            } else {
                *agg.disconnect_reasons
                    .entry("(unspecified)".to_owned())
                    .or_insert(0) += 1;
            }
            // Disconnect events carry the session summary stats.
            if let Some(v) = num_f64(rec.get("ping_average")) {
                agg.ping_sum += v;
                agg.ping_n += 1;
            }
            if let Some(v) = num_f64(rec.get("mos_mean")) {
                agg.mos_sum += v;
                agg.mos_n += 1;
            }
            agg.packets_sent += num_u64(rec.get("packets_sent"));
            agg.packets_lost +=
                num_u64(rec.get("packets_received_lost")) + num_u64(rec.get("packets_sent_lost"));
            agg.connected_ms += num_f64(rec.get("duration_connected_ms")).unwrap_or(0.0);
            agg.speaking_ms += num_f64(rec.get("duration_speaking_ms")).unwrap_or(0.0);
            agg.listening_ms += num_f64(rec.get("duration_listening_ms")).unwrap_or(0.0);
            for key in [
                "duration_connection_type_wifi",
                "duration_connection_type_ethernet",
                "duration_connection_type_cellular",
                "duration_connection_type_bluetooth",
                "duration_connection_type_none",
                "duration_connection_type_other",
                "duration_connection_type_unknown",
            ] {
                if let Some(ms) = num_f64(rec.get(key)) {
                    let label = key.trim_start_matches("duration_connection_type_");
                    *agg.connection_type_ms
                        .entry(label.to_owned())
                        .or_insert(0.0) += ms;
                }
            }
        }
        "media_session_joined" => agg.media_sessions += 1,
        "start_speaking" => agg.speaking_starts += 1,
        "start_listening" => agg.listening_starts += 1,
        _ => {}
    }

    if let Some(host) = pick_str(rec, &["hostname"])
        && host.contains("discord.media")
    {
        *agg.media_hosts.entry(host.to_owned()).or_insert(0) += 1;
    }

    // Device profile (only when present).
    if let Some(os) = pick_str(rec, &["os"]).filter(|s| !s.is_empty()) {
        bump_bounded(&mut agg.by_os, os, 32);
    }
    if let Some(browser) = pick_str(rec, &["browser"]).filter(|s| !s.is_empty()) {
        bump_bounded(&mut agg.by_client, browser, 32);
    }
    if let Some(ch) = pick_str(rec, &["release_channel"]).filter(|s| !s.is_empty()) {
        bump_bounded(&mut agg.by_release_channel, ch, 16);
    }
    if let Some(build) = pick_str(rec, &["client_build_number"]).filter(|s| !s.is_empty()) {
        bump_bounded(&mut agg.by_client_build, build, 200);
    }
    if let Some(isp) = pick_str(rec, &["isp"]).filter(|s| !s.is_empty()) {
        bump_bounded(&mut agg.by_isp, isp, 32);
    }
    if let Some(cc) = pick_str(rec, &["country_code"]).filter(|s| !s.is_empty()) {
        bump_bounded(&mut agg.by_country, cc, 32);
    }
}

fn bump_bounded(map: &mut BTreeMap<String, u64>, key: &str, cap: usize) {
    if map.len() < cap || map.contains_key(key) {
        *map.entry(key.to_owned()).or_insert(0) += 1;
    }
}

fn event_date(rec: &Value) -> String {
    if let Some(d) = pick_str(rec, &["_day_utc"]) {
        let d = d.trim();
        if d.len() >= 10 {
            return d[..10].to_owned();
        }
    }
    if let Some(ts) = pick_str(rec, &["timestamp"]) {
        let ts = ts.trim().trim_matches('"');
        if ts.len() >= 10 {
            return ts[..10].to_owned();
        }
    }
    String::new()
}

fn apply_aggregate(agg: &EventAggregate, insights: &mut Insights) {
    let sessions = &mut insights.sessions;
    sessions.active_days = agg.days.len() as u64;
    sessions.first_active_day = agg.days.keys().next().cloned();
    sessions.last_active_day = agg.days.keys().next_back().cloned();
    sessions.longest_daily_streak = longest_streak(&agg.days);
    sessions.session_starts = agg.session_starts;
    sessions.busiest_day = agg
        .days
        .iter()
        .max_by_key(|(_, c)| **c)
        .map(|(d, c)| (d.clone(), *c));
    sessions.top_event_types = ranked(&agg.by_event_type, 10);

    let voice = &mut insights.voice;
    voice.connections = agg.voice_connections;
    voice.disconnects = agg.voice_disconnects;
    voice.reconnects = agg.reconnects;
    voice.media_sessions = agg.media_sessions;
    voice.speaking_starts = agg.speaking_starts;
    voice.listening_starts = agg.listening_starts;
    if agg.connect_time_n > 0 {
        voice.avg_connect_ms = agg.connect_time_ms_sum / agg.connect_time_n as f64;
    }
    if agg.ping_n > 0 {
        voice.avg_ping_ms = agg.ping_sum / agg.ping_n as f64;
    }
    if agg.mos_n > 0 {
        voice.avg_mos = agg.mos_sum / agg.mos_n as f64;
    }
    voice.packets_sent = agg.packets_sent;
    voice.packets_lost = agg.packets_lost;
    voice.connected_minutes = agg.connected_ms / 60_000.0;
    voice.speaking_minutes = agg.speaking_ms / 60_000.0;
    voice.listening_minutes = agg.listening_ms / 60_000.0;
    voice.minutes_by_connection_type = agg
        .connection_type_ms
        .iter()
        .map(|(k, v)| (k.clone(), v / 60_000.0))
        .collect();
    voice.top_media_hosts = ranked(&agg.media_hosts, 5);
    voice.disconnect_reasons = ranked(&agg.disconnect_reasons, 5);

    let devices = &mut insights.devices;
    devices.by_os = bounded_ranked(&agg.by_os, 8);
    devices.by_client = bounded_ranked(&agg.by_client, 8);
    devices.by_release_channel = bounded_ranked(&agg.by_release_channel, 6);
    devices.by_client_build = bounded_ranked(&agg.by_client_build, 8);
    devices.by_isp = bounded_ranked(&agg.by_isp, 5);
    devices.by_country = bounded_ranked(&agg.by_country, 8);

    // Keep the 24 busiest months in the rendered summary.
    let mut months: Vec<(String, u64)> =
        agg.by_month.iter().map(|(m, c)| (m.clone(), *c)).collect();
    months.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    months.truncate(24);
    sessions.events_by_month = months.into_iter().collect();
}

fn ranked(map: &BTreeMap<String, u64>, n: usize) -> Vec<(String, u64)> {
    let mut v: Vec<(String, u64)> = map.iter().map(|(k, c)| (k.clone(), *c)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v.truncate(n);
    v
}

/// Top-n slice of a count map rendered as an ordered map.
fn bounded_ranked(map: &BTreeMap<String, u64>, n: usize) -> BTreeMap<String, u64> {
    ranked(map, n).into_iter().collect()
}

/// Longest run of consecutive calendar days present in the map.
fn longest_streak(days: &BTreeMap<String, u64>) -> u64 {
    let mut ordinals: Vec<i64> = days.keys().filter_map(|d| day_ordinal(d)).collect();
    ordinals.sort_unstable();
    ordinals.dedup();
    let mut best = 0u64;
    let mut run = 0u64;
    let mut prev: Option<i64> = None;
    for o in ordinals {
        match prev {
            Some(p) if o == p + 1 => run += 1,
            _ => run = 1,
        }
        best = best.max(run);
        prev = Some(o);
    }
    best
}

/// Days since 1970-01-01 for a "YYYY-MM-DD" string (Howard Hinnant's algorithm).
pub fn day_ordinal(date: &str) -> Option<i64> {
    let mut it = date.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

// ---------------------------------------------------------------------------
// summaries/*.json writers
// ---------------------------------------------------------------------------

pub fn write_summaries(results_dir: &Path, insights: &Insights) -> Result<()> {
    fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
        fs::write(path, serde_json::to_string_pretty(value)?)
            .with_context(|| format!("failed to write {}", path.display()))
    }
    let dir = results_dir.join("summaries");
    fs::create_dir_all(&dir)?;
    write_json(&dir.join("billing.json"), &insights.billing)?;
    write_json(&dir.join("privacy.json"), &insights.privacy)?;
    write_json(&dir.join("dms.json"), &insights.social)?;
    write_json(&dir.join("voice.json"), &insights.voice)?;
    write_json(&dir.join("devices.json"), &insights.devices)?;
    write_json(&dir.join("sessions.json"), &insights.sessions)?;
    write_json(&dir.join("links.json"), &insights.links)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// small JSON helpers
// ---------------------------------------------------------------------------

fn str_or(v: Option<&Value>, fallback: &str) -> String {
    v.and_then(value_to_plain_string)
        .unwrap_or_else(|| fallback.to_owned())
}

fn num_i64(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn num_u32(v: Option<&Value>) -> Option<u32> {
    num_i64(v).map(|n| n.clamp(0, u32::MAX as i64) as u32)
}

fn num_u64(v: Option<&Value>) -> u64 {
    match v {
        Some(Value::Number(n)) => n.as_u64().unwrap_or(0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

fn num_f64(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Normalize ISO timestamps that Discord sometimes double-quotes inside strings.
fn normalize_iso(s: String) -> String {
    let s = s.trim().trim_matches('"');
    s.chars().take(10).collect()
}

fn mtime_ms(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// All activity event JSON files under the export's Activity folder, unsorted.
pub fn activity_event_files(activity_dir: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(activity_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| {
            p.extension()
                .map(|ext| ext.to_string_lossy().eq_ignore_ascii_case("json"))
                .unwrap_or(false)
        })
        .collect()
}

/// Stable signature over the given input files (order-independent content:
/// mtime + size). Same formula used to gate the insights cache.
pub fn signature_for(files: &[PathBuf]) -> u64 {
    let mut signature: u64 = 0x9E37_79B9_7F4A_7C15;
    for path in files {
        let meta = fs::metadata(path);
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let mtime = mtime_ms(path);
        signature = signature
            .wrapping_mul(0x100_0000_01B3)
            .wrapping_add(size)
            .wrapping_add(mtime.rotate_left(1));
    }
    signature
}
