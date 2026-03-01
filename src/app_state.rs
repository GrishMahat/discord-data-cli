use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{analyzer, config::AppConfig, downloader, support_activity};
use support_activity::{ActivityEventPreview, SupportTicketView};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct InteractiveSettings {
    pub(crate) download_attachments: bool,
    pub(crate) preview_messages: usize,
}

impl Default for InteractiveSettings {
    fn default() -> Self {
        Self {
            download_attachments: false,
            preview_messages: 40,
        }
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ChannelFilter {
    All,
    Dm,
    GroupDm,
    PublicThread,
    Voice,
}

impl ChannelFilter {
    pub(crate) fn label(self) -> &'static str {
        match self {
            ChannelFilter::All => "All",
            ChannelFilter::Dm => "DMs",
            ChannelFilter::GroupDm => "Group DMs",
            ChannelFilter::PublicThread => "Public Threads",
            ChannelFilter::Voice => "Voice",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Screen {
    Setup,
    Home,
    Overview,
    SupportActivity,
    SupportTicketDetail,
    Activity,
    ActivityDetail,
    ChannelList,
    MessageView,
    Settings,
    Analyzing,
    Downloading,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActivityFilterField {
    Query,
    EventType,
    SourceFile,
    FromDate,
    ToDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActivitySortMode {
    Newest,
    Oldest,
    EventType,
}

impl ActivitySortMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            ActivitySortMode::Newest => "newest",
            ActivitySortMode::Oldest => "oldest",
            ActivitySortMode::EventType => "type",
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            ActivitySortMode::Newest => ActivitySortMode::Oldest,
            ActivitySortMode::Oldest => ActivitySortMode::EventType,
            ActivitySortMode::EventType => ActivitySortMode::Newest,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ActivityFilters {
    pub(crate) query: String,
    pub(crate) event_type: String,
    pub(crate) source_file: String,
    pub(crate) from_date: String,
    pub(crate) to_date: String,
}

pub(crate) const HOME_MENU_ITEMS: [(&str, &str); 12] = [
    ("Analyze Now", "Run full analysis on your Discord export"),
    ("Overview", "View analysis summary and statistics"),
    ("Support Tickets", "Browse support tickets with details"),
    (
        "Activity Explorer",
        "Browse detailed activity with advanced filters and sorting",
    ),
    (
        "Download Attachments",
        "Download media files from your messages",
    ),
    ("Messages (All)", "Browse all message channels"),
    ("DMs", "Browse direct message channels"),
    ("Group DMs", "Browse group direct messages"),
    ("Public Threads", "Browse public thread channels"),
    ("Voice Channels", "Browse voice channel logs"),
    ("Settings", "Configure analyzer options"),
    ("Quit", "Exit the application"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetupStep {
    ExportPath,
    ResultsPath,
    ProfileId,
    Confirm,
}

enum AnalysisEvent {
    Progress(analyzer::AnalysisProgress),
    Finished(std::result::Result<analyzer::AnalysisData, String>),
}

enum DownloadEvent {
    Progress(downloader::DownloadProgress),
    Finished(std::result::Result<(), String>),
}

#[derive(Debug, Clone)]
pub(crate) struct SetupState {
    pub(crate) step: SetupStep,
    pub(crate) input: String,
    pub(crate) export_path: String,
    pub(crate) results_path: String,
    pub(crate) profile_id: String,
    pub(crate) notice: String,
}

impl SetupState {
    fn new(default_export: String) -> Self {
        Self {
            step: SetupStep::ExportPath,
            input: default_export.clone(),
            export_path: default_export,
            results_path: String::new(),
            profile_id: String::new(),
            notice: "Step 1/4: paste Discord export path, then press Enter.".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InteractiveSession {
    config: AppConfig,
    id: String,
    settings: InteractiveSettings,
}

pub(crate) struct AppState {
    pub(crate) config: AppConfig,
    pub(crate) config_path: PathBuf,
    pub(crate) id: String,
    pub(crate) setup: SetupState,
    pub(crate) settings: InteractiveSettings,
    pub(crate) channel_cache: Option<Vec<MessageChannel>>,
    pub(crate) last_data: Option<analyzer::AnalysisData>,
    pub(crate) status: String,
    pub(crate) error: Option<String>,
    pub(crate) analysis_progress: f32,
    pub(crate) analysis_running: bool,
    pub(crate) analysis_started_at: Option<Instant>,
    analysis_rx: Option<Receiver<AnalysisEvent>>,
    pub(crate) download_progress: f32,
    pub(crate) download_running: bool,
    download_rx: Option<Receiver<DownloadEvent>>,
    pub(crate) screen: Screen,
    pub(crate) should_quit: bool,
    pub(crate) animation_tick: u64,
    pub(crate) home_cursor: usize,
    pub(crate) settings_cursor: usize,
    pub(crate) channel_cursor: usize,
    pub(crate) current_filter: ChannelFilter,
    pub(crate) open_channel: Option<MessageChannel>,
    pub(crate) open_message_lines: Vec<String>,
    pub(crate) open_message_scroll: usize,
    pub(crate) support_tickets: Option<Vec<SupportTicketView>>,
    pub(crate) support_ticket_cursor: usize,
    pub(crate) support_ticket_scroll: usize,
    pub(crate) activity_events: Option<Vec<ActivityEventPreview>>,
    pub(crate) activity_cursor: usize,
    pub(crate) activity_filters: ActivityFilters,
    pub(crate) activity_filter_edit: Option<ActivityFilterField>,
    pub(crate) activity_sort: ActivitySortMode,
    pub(crate) activity_detail_scroll: usize,
}

impl AppState {
    pub(crate) fn new(config_path: PathBuf) -> Result<Self> {
        let mut session: Option<InteractiveSession> = None;
        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                if let Ok(parsed) = toml::from_str::<InteractiveSession>(&content) {
                    session = Some(parsed);
                }
            }
        }

        let cwd =
            env::current_dir().with_context(|| "failed to read current directory".to_owned())?;
        let default_export = cwd.display().to_string();

        let mut app = Self {
            config: session
                .as_ref()
                .map(|s| s.config.clone())
                .unwrap_or_default(),
            config_path: config_path.clone(),
            id: session.as_ref().map(|s| s.id.clone()).unwrap_or_default(),
            setup: SetupState::new(default_export),
            settings: session
                .as_ref()
                .map(|s| s.settings.clone())
                .unwrap_or_default(),
            channel_cache: None,
            last_data: None,
            status: "Ready".to_owned(),
            error: None,
            analysis_progress: 0.0,
            analysis_running: false,
            analysis_started_at: None,
            analysis_rx: None,
            download_progress: 0.0,
            download_running: false,
            download_rx: None,
            screen: Screen::Setup,
            should_quit: false,
            animation_tick: 0,
            home_cursor: 0,
            settings_cursor: 0,
            channel_cursor: 0,
            current_filter: ChannelFilter::All,
            open_channel: None,
            open_message_lines: Vec::new(),
            open_message_scroll: 0,
            support_tickets: None,
            support_ticket_cursor: 0,
            support_ticket_scroll: 0,
            activity_events: None,
            activity_cursor: 0,
            activity_filters: ActivityFilters::default(),
            activity_filter_edit: None,
            activity_sort: ActivitySortMode::Newest,
            activity_detail_scroll: 0,
        };

        if session.is_some() {
            let pkg_dir = app.config.package_path(&app.config_path, &app.id);
            if pkg_dir.exists() {
                app.screen = Screen::Home;
                try_load_existing_data(&mut app);
                app.status = "Session loaded. Ready.".to_owned();
            }
        }

        Ok(app)
    }

    pub(crate) fn save_session(&self) {
        let session = InteractiveSession {
            config: self.config.clone(),
            id: self.id.clone(),
            settings: self.settings.clone(),
        };
        if let Ok(content) = toml::to_string_pretty(&session) {
            let _ = fs::write(&self.config_path, content);
        }
    }
}
pub(crate) fn apply_settings_selection(app: &mut AppState) {
    match app.settings_cursor {
        0 => {
            app.settings.download_attachments = !app.settings.download_attachments;
            app.save_session();
        }
        1 => {
            app.settings.preview_messages = (app.settings.preview_messages + 5).min(500);
            app.save_session();
        }
        2 => open_setup_with_current_values(app),
        3 => app.screen = Screen::Home,
        _ => {}
    }
}

pub(crate) fn execute_home_selection(app: &mut AppState) -> Result<()> {
    if let Some(reason) = home_item_disabled_reason(app, app.home_cursor) {
        app.status = reason;
        app.error = None;
        return Ok(());
    }

    match app.home_cursor {
        0 => start_analysis(app),
        1 => {
            try_load_existing_data(app);
            app.screen = Screen::Overview;
        }
        2 => open_support_activity(app)?,
        3 => open_activity(app)?,
        4 => handle_download_attachments(app),
        5 => open_channel_filter(app, ChannelFilter::All)?,
        6 => open_channel_filter(app, ChannelFilter::Dm)?,
        7 => open_channel_filter(app, ChannelFilter::GroupDm)?,
        8 => open_channel_filter(app, ChannelFilter::PublicThread)?,
        9 => open_channel_filter(app, ChannelFilter::Voice)?,
        10 => app.screen = Screen::Settings,
        11 => app.should_quit = true,
        _ => {}
    }

    Ok(())
}

pub(crate) fn home_item_disabled_reason(app: &AppState, idx: usize) -> Option<String> {
    match idx {
        0 | 10 | 11 => {}
        _ if app.last_data.is_none() => {
            return Some(
                "Disabled until analysis is completed. Run 'Analyze Now' first.".to_owned(),
            );
        }
        _ => {}
    }

    let data = app.last_data.as_ref()?;
    match idx {
        2 if !folder_available(data, "support_tickets") => Some(
            "Support Tickets is disabled: source data was not included in this export.".to_owned(),
        ),
        3 if !folder_available(data, "activity") => Some(
            "Activity Explorer is disabled: activity data was not included in this export."
                .to_owned(),
        ),
        4 | 5 | 6 | 7 | 8 | 9 if !folder_available(data, "messages") => Some(
            "Messages features are disabled: messages data was not included in this export."
                .to_owned(),
        ),
        _ => None,
    }
}

pub(crate) fn screen_disabled_reason(app: &AppState, screen: Screen) -> Option<String> {
    match screen {
        Screen::Home
        | Screen::Setup
        | Screen::Settings
        | Screen::Analyzing
        | Screen::Downloading => {
            return None;
        }
        Screen::SupportTicketDetail | Screen::ActivityDetail | Screen::MessageView => return None,
        _ => {}
    }

    if app.last_data.is_none() {
        return Some("Disabled until analysis is completed. Run 'Analyze Now' first.".to_owned());
    }

    let data = app.last_data.as_ref()?;
    match screen {
        Screen::Overview => None,
        Screen::SupportActivity if !folder_available(data, "support_tickets") => {
            Some("Support is disabled: source data was not included in this export.".to_owned())
        }
        Screen::Activity if !folder_available(data, "activity") => {
            Some("Activity is disabled: source data was not included in this export.".to_owned())
        }
        Screen::ChannelList if !folder_available(data, "messages") => {
            Some("Channels is disabled: messages data was not included in this export.".to_owned())
        }
        _ => None,
    }
}

pub(crate) fn setup_submit_step(app: &mut AppState) -> Result<()> {
    match app.setup.step {
        SetupStep::ExportPath => {
            let raw = app.setup.input.trim();
            if raw.is_empty() {
                bail!("Export path is required.");
            }

            let export_dir = to_absolute(PathBuf::from(raw))?;
            if !export_dir.is_dir() {
                bail!("Export path not found: {}", export_dir.display());
            }

            app.setup.export_path = export_dir.display().to_string();
            app.setup.results_path = export_dir.join("results-rs").display().to_string();
            app.setup.step = SetupStep::ResultsPath;
            app.setup.input = app.setup.results_path.clone();
            app.setup.notice = "Step 2/4: choose results directory and press Enter.".to_owned();
        }
        SetupStep::ResultsPath => {
            let raw = app.setup.input.trim();
            let selected = if raw.is_empty() {
                PathBuf::from(&app.setup.results_path)
            } else {
                to_absolute(PathBuf::from(raw))?
            };

            if selected.exists() && !selected.is_dir() {
                bail!(
                    "Results path exists but is not a directory: {}",
                    selected.display()
                );
            }

            app.setup.results_path = selected.display().to_string();
            app.setup.step = SetupStep::ProfileId;
            app.setup.input = app.setup.profile_id.clone();
            app.setup.notice = "Step 3/4: optional profile ID, then Enter.".to_owned();
        }
        SetupStep::ProfileId => {
            app.setup.profile_id = app.setup.input.trim().to_owned();
            app.setup.step = SetupStep::Confirm;
            app.setup.input.clear();
            app.setup.notice = "Step 4/4: review values and press Enter.".to_owned();
        }
        SetupStep::Confirm => apply_setup(app)?,
    }

    Ok(())
}

pub(crate) fn setup_prev_step(app: &mut AppState) {
    match app.setup.step {
        SetupStep::ExportPath => {}
        SetupStep::ResultsPath => {
            app.setup.step = SetupStep::ExportPath;
            app.setup.input = app.setup.export_path.clone();
            app.setup.notice = "Step 1/4: edit export path and press Enter.".to_owned();
        }
        SetupStep::ProfileId => {
            app.setup.step = SetupStep::ResultsPath;
            app.setup.input = app.setup.results_path.clone();
            app.setup.notice = "Step 2/4: edit results directory and press Enter.".to_owned();
        }
        SetupStep::Confirm => {
            app.setup.step = SetupStep::ProfileId;
            app.setup.input = app.setup.profile_id.clone();
            app.setup.notice = "Step 3/4: edit profile ID and press Enter.".to_owned();
        }
    }
}

fn apply_setup(app: &mut AppState) -> Result<()> {
    let package_raw = app.setup.export_path.trim();
    if package_raw.is_empty() {
        bail!("Export path is required.");
    }
    let package_dir = to_absolute(PathBuf::from(package_raw))?;
    if !package_dir.is_dir() {
        bail!("Export path not found: {}", package_dir.display());
    }

    let results_raw = app.setup.results_path.trim();
    if results_raw.is_empty() {
        bail!("Results directory is required.");
    }
    let results_dir = to_absolute(PathBuf::from(results_raw))?;
    if results_dir.exists() && !results_dir.is_dir() {
        bail!(
            "Results path exists but is not a directory: {}",
            results_dir.display()
        );
    }
    if !results_dir.exists() {
        fs::create_dir_all(&results_dir)
            .with_context(|| format!("failed to create {}", results_dir.display()))?;
    }

    app.config.package_directory = package_dir.display().to_string();
    app.config.results_directory = results_dir.display().to_string();
    app.id = app.setup.profile_id.trim().to_owned();
    app.save_session();
    app.screen = Screen::Home;
    app.home_cursor = 0;
    app.channel_cache = None;
    app.open_channel = None;
    app.open_message_lines.clear();
    app.open_message_scroll = 0;
    app.support_tickets = None;
    app.support_ticket_cursor = 0;
    app.support_ticket_scroll = 0;
    app.activity_events = None;
    app.activity_cursor = 0;
    app.activity_filters = ActivityFilters::default();
    app.activity_filter_edit = None;
    app.activity_sort = ActivitySortMode::Newest;
    app.activity_detail_scroll = 0;
    app.setup.notice = "Setup complete. Ready to analyze.".to_owned();
    app.status = "Setup complete. Ready.".to_owned();
    app.error = None;
    try_load_existing_data(app);
    Ok(())
}

fn open_setup_with_current_values(app: &mut AppState) {
    app.setup.export_path = app
        .config
        .package_path(&app.config_path, &app.id)
        .display()
        .to_string();
    app.setup.results_path = app
        .config
        .results_path(&app.config_path, &app.id)
        .display()
        .to_string();
    app.setup.profile_id = app.id.clone();
    app.setup.step = SetupStep::ExportPath;
    app.setup.input = app.setup.export_path.clone();
    app.setup.notice = "Step 1/4: edit export path and press Enter.".to_owned();
    app.screen = Screen::Setup;
}

pub(crate) fn switch_filter(app: &mut AppState, filter: ChannelFilter) -> Result<()> {
    app.current_filter = filter;
    app.channel_cursor = 0;
    ensure_channels_loaded(app)?;
    Ok(())
}

pub(crate) fn open_channel_filter(app: &mut AppState, filter: ChannelFilter) -> Result<()> {
    app.current_filter = filter;
    app.channel_cursor = 0;
    ensure_channels_loaded(app)?;
    app.screen = Screen::ChannelList;
    Ok(())
}

pub(crate) fn open_support_activity(app: &mut AppState) -> Result<()> {
    try_load_existing_data(app);

    if app.support_tickets.is_none() || app.activity_events.is_none() {
        if let Err(err) = refresh_support_activity_data(app) {
            app.status = "Support data loaded with warnings.".to_owned();
            app.error = Some(err.to_string());
        }
    }

    app.screen = Screen::SupportActivity;
    Ok(())
}

pub(crate) fn open_activity(app: &mut AppState) -> Result<()> {
    try_load_existing_data(app);

    if app.activity_events.is_none() {
        if let Err(err) = refresh_support_activity_data(app) {
            app.status = "Activity loaded with warnings.".to_owned();
            app.error = Some(err.to_string());
        }
    }

    app.screen = Screen::Activity;
    Ok(())
}

pub(crate) fn refresh_support_activity_data(app: &mut AppState) -> Result<()> {
    let package_dir = app.config.package_path(&app.config_path, &app.id);
    let tickets = support_activity::load_support_tickets(&package_dir, &app.config.source_aliases)?;
    let events = support_activity::load_recent_activity_events(
        &package_dir,
        &app.config.source_aliases,
        250,
    )?;

    app.support_tickets = Some(tickets);
    app.activity_events = Some(events);

    let ticket_count = app.support_tickets.as_ref().map(|v| v.len()).unwrap_or(0);
    if ticket_count == 0 {
        app.support_ticket_cursor = 0;
        app.support_ticket_scroll = 0;
    } else {
        app.support_ticket_cursor = app.support_ticket_cursor.min(ticket_count - 1);
        app.support_ticket_scroll = 0;
    }

    let event_count = app.activity_events.as_ref().map(|v| v.len()).unwrap_or(0);
    app.activity_cursor = 0;
    app.activity_filter_edit = None;
    app.activity_sort = ActivitySortMode::Newest;
    app.activity_detail_scroll = 0;
    app.status = format!(
        "Support & Activity refreshed: {} tickets, {} recent events",
        fmt_num(ticket_count as u64),
        fmt_num(event_count as u64)
    );
    app.error = None;

    Ok(())
}

pub(crate) fn open_selected_support_ticket(app: &mut AppState) {
    if app
        .support_tickets
        .as_ref()
        .is_some_and(|tickets| app.support_ticket_cursor < tickets.len())
    {
        app.support_ticket_scroll = 0;
        app.screen = Screen::SupportTicketDetail;
    }
}

pub(crate) fn open_selected_activity_event(app: &mut AppState) {
    let events = filtered_activity_events(app);
    if app.activity_cursor < events.len() {
        app.activity_detail_scroll = 0;
        app.screen = Screen::ActivityDetail;
    }
}

pub(crate) fn open_selected_channel(app: &mut AppState) -> Result<()> {
    let selected = {
        let channels = filtered_channels(app);
        channels
            .get(app.channel_cursor)
            .map(|channel| (*channel).clone())
    };

    if let Some(channel) = selected {
        app.open_message_lines = load_message_preview(&channel, app.settings.preview_messages)?;
        app.open_channel = Some(channel);
        app.open_message_scroll = 0;
        app.screen = Screen::MessageView;
    }

    Ok(())
}

pub(crate) fn key_help(screen: Screen) -> &'static str {
    match screen {
        Screen::Setup => "type/paste value  enter: next  left/up: back  esc: quit",
        Screen::Home => {
            "tab/shift-tab: switch tab  w/s/↑↓: move  enter: select  1-9: quick pick  click: select  q: quit"
        }
        Screen::Overview => "tab/shift-tab: switch tab  r: refresh  b/esc: back  q: quit",
        Screen::SupportActivity => {
            "tab/shift-tab: switch tab  ↑↓: select ticket  enter: open  r: reload  b/esc: back  q: quit"
        }
        Screen::SupportTicketDetail => "↑↓/k/j: scroll  pgup/dn: page  b/esc: back",
        Screen::Activity => {
            "tab/shift-tab: switch tab  ↑↓: browse  enter: open  / t y [ ]: filters  o: sort  c: clear  r: reload  b/esc: back  q: quit"
        }
        Screen::ActivityDetail => "↑↓/k/j: scroll  pgup/dn: page  b/esc: back",
        Screen::ChannelList => "w/s/↑↓: move  u/d pgup/dn: page  1-5: filter  enter: open  b: back",
        Screen::MessageView => "↑↓/k/j: scroll  pgup/dn: page  b/esc: back",
        Screen::Settings => {
            "tab/shift-tab: switch tab  w/s/↑↓: move  ←→: adjust  enter: toggle/apply  b/esc: back  q: quit"
        }
        Screen::Analyzing | Screen::Downloading => "Please wait...",
    }
}

pub(crate) fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

pub(crate) fn try_load_existing_data(app: &mut AppState) {
    let results_dir = app.config.results_path(&app.config_path, &app.id);
    if let Ok(data) = analyzer::read_data(&results_dir) {
        app.last_data = data;
    }
}

pub(crate) fn start_analysis(app: &mut AppState) {
    if app.analysis_running {
        return;
    }

    app.error = None;
    app.status = "Preparing analysis...".to_owned();
    app.analysis_progress = 0.0;
    app.analysis_running = true;
    app.analysis_started_at = Some(Instant::now());
    app.screen = Screen::Analyzing;

    let (tx, rx) = mpsc::channel();
    let config = app.config.clone();
    let config_path = app.config_path.clone();
    let id = app.id.clone();

    thread::spawn(move || {
        let result = analyzer::run_with_progress(&config, &config_path, &id, |progress| {
            let _ = tx.send(AnalysisEvent::Progress(progress));
        })
        .map_err(|err| err.to_string());
        let _ = tx.send(AnalysisEvent::Finished(result));
    });

    app.analysis_rx = Some(rx);
}

pub(crate) fn poll_analysis(app: &mut AppState) {
    let mut finished = false;
    let mut disconnected = false;

    if let Some(rx) = app.analysis_rx.as_ref() {
        loop {
            match rx.try_recv() {
                Ok(AnalysisEvent::Progress(progress)) => {
                    app.analysis_progress = progress.fraction.clamp(0.0, 1.0);
                    app.status = progress.label;
                }
                Ok(AnalysisEvent::Finished(Ok(data))) => {
                    app.analysis_running = false;
                    app.analysis_started_at = None;
                    app.analysis_progress = 1.0;
                    app.status = "Analysis finished successfully.".to_owned();
                    app.error = None;

                    let links = data.messages.attachment_links.clone();
                    app.last_data = Some(data);
                    app.channel_cache = None;
                    app.support_tickets = None;
                    app.activity_events = None;
                    finished = true;

                    if app.settings.download_attachments && !links.is_empty() {
                        start_download(app, links);
                    } else {
                        app.screen = Screen::Overview;
                    }

                    break;
                }
                Ok(AnalysisEvent::Finished(Err(err))) => {
                    app.analysis_running = false;
                    app.analysis_started_at = None;
                    app.analysis_progress = 0.0;
                    app.error = Some(err.clone());
                    app.status = "Analysis failed.".to_owned();
                    app.screen = Screen::Home;
                    finished = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
    }

    if finished || disconnected {
        app.analysis_rx = None;
    }

    if disconnected {
        app.analysis_running = false;
        app.analysis_started_at = None;
        app.analysis_progress = 0.0;
        app.error = Some("analysis thread disconnected unexpectedly".to_owned());
        app.status = "Analysis failed.".to_owned();
        app.screen = Screen::Home;
    }
}

pub(crate) fn handle_download_attachments(app: &mut AppState) {
    if app.download_running {
        app.status = "Download already running.".to_owned();
        return;
    }

    if app.last_data.is_none() {
        try_load_existing_data(app);
    }

    if let Some(data) = &app.last_data {
        if data.messages.attachment_links.is_empty() {
            app.status = "No attachments found in the analysis data.".to_owned();
        } else {
            start_download(app, data.messages.attachment_links.clone());
        }
    } else {
        app.status = "No analysis data found. Run Analysis first.".to_owned();
    }
}

fn start_download(app: &mut AppState, links: Vec<String>) {
    if app.download_running {
        return;
    }

    app.error = None;
    app.status = "Starting attachment download...".to_owned();
    app.download_progress = 0.0;
    app.download_running = true;
    app.screen = Screen::Downloading;

    let (tx, rx) = mpsc::channel();
    let results_dir = app.config.results_path(&app.config_path, &app.id);

    thread::spawn(move || {
        let tx2 = tx.clone();
        let result = downloader::download_attachments(&results_dir, links, move |progress| {
            let _ = tx2.send(DownloadEvent::Progress(progress));
        })
        .map_err(|err| err.to_string());
        let _ = tx.send(DownloadEvent::Finished(result));
    });

    app.download_rx = Some(rx);
}

pub(crate) fn poll_download(app: &mut AppState) {
    let mut finished = false;
    let mut disconnected = false;

    if let Some(rx) = app.download_rx.as_ref() {
        loop {
            match rx.try_recv() {
                Ok(DownloadEvent::Progress(progress)) => {
                    app.download_progress = progress.fraction.clamp(0.0, 1.0);
                    app.status = progress.label;
                }
                Ok(DownloadEvent::Finished(Ok(()))) => {
                    app.download_running = false;
                    app.download_progress = 1.0;
                    app.status = "Download finished successfully.".to_owned();
                    app.error = None;
                    app.screen = Screen::Overview;
                    finished = true;
                    break;
                }
                Ok(DownloadEvent::Finished(Err(err))) => {
                    app.download_running = false;
                    app.download_progress = 0.0;
                    app.error = Some(err.clone());
                    app.status = "Download failed.".to_owned();
                    app.screen = Screen::Home;
                    finished = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
    }

    if finished || disconnected {
        app.download_rx = None;
    }

    if disconnected {
        app.download_running = false;
        app.download_progress = 0.0;
        app.error = Some("download thread disconnected unexpectedly".to_owned());
        app.status = "Download failed.".to_owned();
        app.screen = Screen::Home;
    }
}

pub(crate) fn filtered_channels(app: &AppState) -> Vec<&MessageChannel> {
    let Some(channels) = app.channel_cache.as_ref() else {
        return Vec::new();
    };

    channels
        .iter()
        .filter(|channel| match app.current_filter {
            ChannelFilter::All => true,
            ChannelFilter::Dm => channel.kind == ChannelKind::Dm,
            ChannelFilter::GroupDm => channel.kind == ChannelKind::GroupDm,
            ChannelFilter::PublicThread => channel.kind == ChannelKind::PublicThread,
            ChannelFilter::Voice => channel.kind == ChannelKind::Voice,
        })
        .collect()
}

fn ensure_channels_loaded(app: &mut AppState) -> Result<()> {
    if app.channel_cache.is_some() {
        return Ok(());
    }

    app.status = "Loading channel index...".to_owned();

    let package_dir = app.config.package_path(&app.config_path, &app.id);
    let Some(messages_dir) =
        resolve_optional_subdir(&package_dir, &app.config.source_aliases.messages)?
    else {
        app.channel_cache = Some(Vec::new());
        app.status = "Messages directory not found".to_owned();
        return Ok(());
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

    app.channel_cache = Some(channels);
    app.status = "Channel index loaded".to_owned();

    Ok(())
}

fn load_message_preview(channel: &MessageChannel, preview_count: usize) -> Result<Vec<String>> {
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

fn to_absolute(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path);
    }

    let cwd = env::current_dir().with_context(|| "failed to read current directory".to_owned())?;
    Ok(cwd.join(path))
}

fn folder_available(data: &analyzer::AnalysisData, key: &str) -> bool {
    if data.folder_presence.is_empty() {
        return true;
    }
    data.folder_presence.get(key).copied().unwrap_or(true)
}

pub(crate) fn ratio(part: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 / total as f64
    }
}

pub(crate) fn fmt_num(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

pub(crate) fn truncate_text(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let kept = max_chars.saturating_sub(1);
    format!("{}…", text.chars().take(kept).collect::<String>())
}

pub(crate) fn top_counts(map: &BTreeMap<String, u64>, limit: usize) -> Vec<(String, u64)> {
    let mut items: Vec<(String, u64)> = map.iter().map(|(k, v)| (k.clone(), *v)).collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    items.truncate(limit);
    items
}

pub(crate) fn is_printable_input(c: char) -> bool {
    c.is_ascii() && !c.is_ascii_control()
}

pub(crate) fn filtered_activity_events(app: &AppState) -> Vec<ActivityEventPreview> {
    let Some(events) = app.activity_events.as_ref() else {
        return Vec::new();
    };

    let mut out: Vec<ActivityEventPreview> = events
        .iter()
        .filter(|event| activity_event_matches_filters(event, &app.activity_filters))
        .cloned()
        .collect();

    match app.activity_sort {
        ActivitySortMode::Newest => {
            out.sort_by(|a, b| {
                b.sort_key
                    .cmp(&a.sort_key)
                    .then_with(|| a.event_type.cmp(&b.event_type))
            });
        }
        ActivitySortMode::Oldest => {
            out.sort_by(|a, b| {
                a.sort_key
                    .cmp(&b.sort_key)
                    .then_with(|| a.event_type.cmp(&b.event_type))
            });
        }
        ActivitySortMode::EventType => {
            out.sort_by(|a, b| {
                a.event_type
                    .cmp(&b.event_type)
                    .then_with(|| b.sort_key.cmp(&a.sort_key))
            });
        }
    }

    out
}

fn activity_event_matches_filters(event: &ActivityEventPreview, filters: &ActivityFilters) -> bool {
    let query = filters.query.trim();
    if !query.is_empty() {
        let needle = query.to_ascii_lowercase();
        let haystack = format!(
            "{} {} {} {}",
            event.timestamp, event.event_type, event.summary, event.source_file
        )
        .to_ascii_lowercase();
        if !haystack.contains(&needle) {
            return false;
        }
    }

    let event_type = filters.event_type.trim();
    if !event_type.is_empty()
        && !event
            .event_type
            .to_ascii_lowercase()
            .contains(&event_type.to_ascii_lowercase())
    {
        return false;
    }

    let source_file = filters.source_file.trim();
    if !source_file.is_empty()
        && !event
            .source_file
            .to_ascii_lowercase()
            .contains(&source_file.to_ascii_lowercase())
    {
        return false;
    }

    let from_date = normalize_date_filter(&filters.from_date);
    if let Some(from_date) = from_date.as_deref() {
        let Some(event_date) = event.date_key.as_deref() else {
            return false;
        };
        if event_date < from_date {
            return false;
        }
    }

    let to_date = normalize_date_filter(&filters.to_date);
    if let Some(to_date) = to_date.as_deref() {
        let Some(event_date) = event.date_key.as_deref() else {
            return false;
        };
        if event_date > to_date {
            return false;
        }
    }

    true
}

fn normalize_date_filter(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let bytes = trimmed.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    let date = &bytes[0..10];
    if date[0..4].iter().all(u8::is_ascii_digit)
        && date[4] == b'-'
        && date[5..7].iter().all(u8::is_ascii_digit)
        && date[7] == b'-'
        && date[8..10].iter().all(u8::is_ascii_digit)
    {
        return Some(String::from_utf8_lossy(date).to_string());
    }
    None
}
