use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

pub mod report;
pub mod structs;
pub mod workers;

pub use structs::AnalysisData;

// The many steps of analysis. Like grief, but for data processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalysisStep {
    Preparing,
    Account,
    Messages,
    Servers,
    Support,
    Activity,
    Activities,
    Programs,
    Writing,
    Complete,
}

impl AnalysisStep {
    pub fn label(self) -> &'static str {
        match self {
            AnalysisStep::Preparing => "Preparing",   // Shaving the yak
            AnalysisStep::Account => "Account",       // Who even are you?
            AnalysisStep::Messages => "Messages",     // The main event
            AnalysisStep::Servers => "Servers",       // The places you called home
            AnalysisStep::Support => "Support",       // Your complaints to Discord HQ
            AnalysisStep::Activity => "Activity",     // What did you do, when?
            AnalysisStep::Activities => "Activities", // Programs. Games. Productivity.
            AnalysisStep::Programs => "Programs",
            AnalysisStep::Writing => "Writing results", // Pen to paper, or JSON to disk
            AnalysisStep::Complete => "Complete",       // The end. Congratulations.
        }
    }
}

// Progress updates for the UI. "We're still going. Have faith."
#[derive(Debug, Clone)]
pub struct AnalysisProgress {
    pub fraction: f32,
    pub step: AnalysisStep,
    pub label: String,
    pub current_file: Option<String>,
    pub files_processed: Option<u32>,
    pub total_files: Option<u32>,
}

// The main analysis function. It coordinates everything. Like a project manager, but faster.
pub fn run_with_progress<F>(
    config: &crate::config::AppConfig,
    config_path: &Path,
    id: &str,
    abort: Arc<AtomicBool>,
    mut on_progress: F,
) -> Result<AnalysisData>
where
    F: FnMut(AnalysisProgress),
{
    // 9 steps to enlightenment (or data overload). Let's not count the 0th step.
    const TOTAL_STEPS: f32 = 9.0;
    let step_frac = |s: f32| (s / TOTAL_STEPS).clamp(0.0, 1.0);

    // Give me a reason to stop playing god and end this thread right now.
    let check_abort = || {
        if abort.load(Ordering::SeqCst) {
            bail!("Analysis canceled by user.");
        }
        Ok(())
    };

    check_abort()?;
    // "We are entering the matrix..."
    emit(
        &mut on_progress,
        step_frac(0.0),
        AnalysisStep::Preparing,
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
    fs::create_dir_all(&results_dir)?;

    // Where on earth did Discord hide everything? Ah, here they are.
    let source_dirs = SourceDirs::discover(&package_dir, &config.source_aliases)?;
    let existing = report::read_data(&results_dir).ok().flatten();
    let mut stats = AnalysisData {
        meta: structs::Meta {
            tool_version: env!("CARGO_PKG_VERSION").to_owned(),
            analyzed_at: utc_now_iso8601(),
            package_directory: package_dir.display().to_string(),
            results_directory: results_dir.display().to_string(),
        },
        folder_presence: source_dirs.presence_map(),
        package_directory: package_dir.display().to_string(),
        results_directory: results_dir.display().to_string(),
        channels_cache: existing
            .as_ref()
            .map(|d| d.channels_cache.clone())
            .unwrap_or_default(),
        activity_cache: existing
            .as_ref()
            .map(|d| d.activity_cache.clone())
            .unwrap_or_default(),
        ..AnalysisData::default()
    };

    check_abort()?;
    emit(
        &mut on_progress,
        step_frac(1.0),
        AnalysisStep::Account,
        "Analyzing account...",
    );
    if let Some(d) = &source_dirs.account {
        workers::analyze_account(d, &mut stats)?;
    } else {
        stats.warnings.push("Account directory missing.".to_owned());
    }

    emit(
        &mut on_progress,
        step_frac(2.0),
        AnalysisStep::Messages,
        "Analyzing messages...",
    );
    workers::analyze_messages(source_dirs.messages.as_deref(), &mut stats)?;
    emit(
        &mut on_progress,
        step_frac(3.0),
        AnalysisStep::Servers,
        "Analyzing servers...",
    );
    workers::analyze_servers(source_dirs.servers.as_deref(), &mut stats)?;
    emit(
        &mut on_progress,
        step_frac(4.0),
        AnalysisStep::Support,
        "Analyzing support tickets...",
    );
    workers::analyze_support_tickets(source_dirs.support_tickets.as_deref(), &mut stats)?;

    emit(
        &mut on_progress,
        step_frac(5.0),
        AnalysisStep::Activity,
        "Analyzing activity events...",
    );
    check_abort()?;
    workers::analyze_activity(source_dirs.activity.as_deref(), &mut stats, |f, d| {
        emit(
            &mut on_progress,
            step_frac(5.0 + f),
            AnalysisStep::Activity,
            format!("Analyzing activity events... {d}"),
        );
    })?;

    emit(
        &mut on_progress,
        step_frac(6.0),
        AnalysisStep::Activities,
        "Analyzing activities...",
    );
    workers::analyze_activities(source_dirs.activities.as_deref(), &mut stats)?;
    emit(
        &mut on_progress,
        step_frac(7.0),
        AnalysisStep::Programs,
        "Analyzing programs...",
    );
    workers::analyze_programs(source_dirs.programs.as_deref(), &mut stats)?;

    emit(
        &mut on_progress,
        step_frac(8.0),
        AnalysisStep::Writing,
        "Writing results...",
    );
    let data_path = results_dir.join("data.json");
    fs::write(&data_path, serde_json::to_string_pretty(&stats)?)?;
    let _ = fs::write(
        results_dir.join("report.md"),
        report::generate_markdown_report(&stats),
    );

    emit(
        &mut on_progress,
        1.0,
        AnalysisStep::Complete,
        "Analysis complete. You did it!",
    );
    Ok(stats)
}

fn emit<F>(on_progress: &mut F, fraction: f32, step: AnalysisStep, label: impl Into<String>)
where
    F: FnMut(AnalysisProgress),
{
    on_progress(AnalysisProgress {
        fraction,
        step,
        label: label.into(),
        current_file: None,
        files_processed: None,
        total_files: None,
    });
}

#[allow(dead_code)]
fn emit_file<F>(
    on_progress: &mut F,
    fraction: f32,
    step: AnalysisStep,
    label: &str,
    current_file: &str,
    files_processed: u32,
    total_files: u32,
) where
    F: FnMut(AnalysisProgress),
{
    on_progress(AnalysisProgress {
        fraction,
        step,
        label: label.into(),
        current_file: Some(current_file.to_owned()),
        files_processed: Some(files_processed),
        total_files: Some(total_files),
    });
}

pub fn read_data(results_dir: &Path) -> Result<Option<AnalysisData>> {
    report::read_data(results_dir)
}

// The source directories structure. Where each type of data is located.
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
    fn discover(package_dir: &Path, aliases: &crate::config::SourceAliases) -> Result<Self> {
        use crate::data::utils::resolve_optional_subdir;
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
    // Did the user delete half the directories? Let's check.
    fn presence_map(&self) -> BTreeMap<String, bool> {
        [
            ("account", self.account.is_some()),
            ("activity", self.activity.is_some()),
            ("activities", self.activities.is_some()),
            ("messages", self.messages.is_some()),
            ("programs", self.programs.is_some()),
            ("servers", self.servers.is_some()),
            ("support_tickets", self.support_tickets.is_some()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
    }
}

// Because dealing with actual time libraries is for the weak. We do math!
fn utc_now_iso8601() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (year, month, day) = days_to_ymd(now / 86400);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        (now / 3600) % 24,
        (now / 60) % 60,
        now % 60
    )
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970;
    while days >= if is_leap(year) { 366 } else { 365 } {
        days -= if is_leap(year) { 366 } else { 365 };
        year += 1;
    }
    let mdays = [
        31,
        if is_leap(year) { 29 } else { 28 },
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
    let mut month = 1;
    for &d in &mdays {
        if days < d {
            break;
        }
        days -= d;
        month += 1;
    }
    (year, month, days + 1)
}

// Fun fact: The universe has leap years to keep developers employed.
fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
