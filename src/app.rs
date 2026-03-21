use std::{
    collections::BTreeMap,
    env, fs,
    path::PathBuf,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use std::io::Write;
use crate::{analyzer, config::AppConfig, data, downloader};

fn log_msg(msg: &str) {
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/discord-cli.log")
    {
        let now = SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
        let _ = writeln!(file, "[{}] {}", now, msg);
    }
}
use data::{ActivityEventPreview, SupportTicketView};

pub(crate) use data::{ChannelKind, MessageChannel};

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
    Gallery,
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

pub(crate) const HOME_MENU_ITEMS: [(&str, &str); 13] = [
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
    ("Gallery", "Browse and search through downloaded attachments"),
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
    Finished(Box<std::result::Result<analyzer::AnalysisData, String>>),
}

enum SupportActivityEvent {
    Finished(std::result::Result<(Vec<SupportTicketView>, Vec<ActivityEventPreview>), String>),
}

enum GalleryEvent {
    Finished(std::result::Result<Vec<AttachmentFile>, String>),
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
    pub(crate) gallery: GalleryState,
    pub(crate) last_data_mtime: u64,
    pub(crate) support_activity_loading: bool,
    support_activity_rx: Option<Receiver<SupportActivityEvent>>,
    pub(crate) gallery_loading: bool,
    gallery_rx: Option<Receiver<GalleryEvent>>,
}

#[derive(Debug, Clone)]
pub(crate) struct GalleryState {
    pub(crate) files: Vec<AttachmentFile>,
    pub(crate) cursor: usize,
    pub(crate) scroll: usize,
    pub(crate) category_filter: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AttachmentFile {
    pub(crate) name: String,
    pub(crate) _path: PathBuf,
    pub(crate) size: u64,
    pub(crate) category: String,
}

impl AppState {
    pub(crate) fn new(config_path: PathBuf) -> Result<Self> {
        let mut session: Option<InteractiveSession> = None;
        if config_path.exists()
            && let Ok(content) = fs::read_to_string(&config_path)
            && let Ok(parsed) = toml::from_str::<InteractiveSession>(&content)
        {
            session = Some(parsed);
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
            gallery: GalleryState {
                files: Vec::new(),
                cursor: 0,
                scroll: 0,
                category_filter: None,
            },
            last_data_mtime: 0,
            support_activity_loading: false,
            support_activity_rx: None,
            gallery_loading: false,
            gallery_rx: None,
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
        5 => open_gallery(app)?,
        6 => open_channel_filter(app, ChannelFilter::Dm)?, // DMs & Groups
        7 => open_channel_filter(app, ChannelFilter::PublicThread)?, // Threads / Voice
        8 => open_channel_filter(app, ChannelFilter::All)?, // Full Archive
        9 => app.status = "Export features coming soon.".to_owned(), // Export to CSV
        10 => app.screen = Screen::Settings,
        11 => app.status = "Help docs coming soon. Use Tab, arrows, and Enter.".to_owned(),
        12 => app.should_quit = true,
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
        4 if !folder_available(data, "messages") => Some(
            "Attachment Downloader is disabled: messages data was not included in this export."
                .to_owned(),
        ),
        5 if !folder_available(data, "messages") => Some(
            "Gallery is disabled: messages data was not included in this export."
                .to_owned(),
        ),
        6..=10 if !folder_available(data, "messages") => Some(
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
        Screen::ChannelList | Screen::Gallery if !folder_available(data, "messages") => {
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
        start_support_activity_load(app);
    }

    app.screen = Screen::SupportActivity;
    Ok(())
}

pub(crate) fn open_activity(app: &mut AppState) -> Result<()> {
    try_load_existing_data(app);

    if app.activity_events.is_none() {
        start_support_activity_load(app);
    }

    app.screen = Screen::Activity;
    Ok(())
}

pub(crate) fn start_support_activity_load(app: &mut AppState) {
    if app.support_activity_loading {
        return;
    }

    app.support_activity_loading = true;
    app.status = "Loading support/activity data in background...".to_owned();
    
    let (tx, rx) = mpsc::channel();
    let package_dir = app.config.package_path(&app.config_path, &app.id);
    let aliases = app.config.source_aliases.clone();
    
    thread::spawn(move || {
        let tickets_res = data::load_support_tickets(&package_dir, &aliases);
        let activity_res = data::load_recent_activity_events(&package_dir, &aliases, 250);
        
        let result = match (tickets_res, activity_res) {
            (Ok(t), Ok(a)) => Ok((t, a)),
            (Err(e), _) => Err(e.to_string()),
            (_, Err(e)) => Err(e.to_string()),
        };
        
        let _ = tx.send(SupportActivityEvent::Finished(result));
    });
    
    app.support_activity_rx = Some(rx);
}

pub(crate) fn poll_support_activity(app: &mut AppState) {
    if let Some(rx) = &app.support_activity_rx {
        match rx.try_recv() {
            Ok(SupportActivityEvent::Finished(result)) => {
                app.support_activity_loading = false;
                match result {
                    Ok((tickets, events)) => {
                        app.support_tickets = Some(tickets);
                        app.activity_events = Some(events);
                        app.status = "Support/activity data loaded.".to_owned();
                    }
                    Err(e) => {
                        app.error = Some(format!("Failed to load data: {}", e));
                        app.status = "Support/activity load failed.".to_owned();
                    }
                }
                app.support_activity_rx = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                app.support_activity_loading = false;
                app.support_activity_rx = None;
            }
        }
    }
}

pub(crate) fn refresh_support_activity_data(app: &mut AppState) -> Result<()> {
    let package_dir = app.config.package_path(&app.config_path, &app.id);
    
    let start = Instant::now();
    log_msg("Refreshing support/activity data...");
    
    let tickets = data::load_support_tickets(&package_dir, &app.config.source_aliases)?;
    log_msg(&format!("Tickets loaded: {} in {:?}", tickets.len(), start.elapsed()));
    
    let activity_start = Instant::now();
    let events = data::load_recent_activity_events(&package_dir, &app.config.source_aliases, 250)?;
    log_msg(&format!("Recent activity loaded: {} in {:?}", events.len(), activity_start.elapsed()));
    
    app.support_tickets = Some(tickets);
    app.activity_events = Some(events);
    app.status = format!("Loaded {} tickets and {} events", 
        app.support_tickets.as_ref().map(|v| v.len()).unwrap_or(0),
        app.activity_events.as_ref().map(|v| v.len()).unwrap_or(0)
    );
    
    Ok(())
}

pub(crate) fn open_gallery(app: &mut AppState) -> Result<()> {
    if app.gallery.files.is_empty() {
        start_gallery_load(app);
    }
    app.screen = Screen::Gallery;
    app.gallery.cursor = 0;
    app.gallery.scroll = 0;
    Ok(())
}

pub(crate) fn start_gallery_load(app: &mut AppState) {
    if app.gallery_loading {
        return;
    }
    
    app.gallery_loading = true;
    app.status = "Scanning attachments in background...".to_owned();
    
    let (tx, rx) = mpsc::channel();
    let config = app.config.clone();
    let config_path = app.config_path.clone();
    let id = app.id.clone();
    
    thread::spawn(move || {
        let results_dir = config.results_path(&config_path, &id);
        let mut files = Vec::new();
        
        if results_dir.exists() {
            let categories = [
                "imgs", "vids", "audios", "docs", "txts", "codes", "data", "exes", "zips", "unknowns",
            ];
            
            for cat in categories {
                let cat_dir = results_dir.join(cat);
                if cat_dir.is_dir() {
                    if let Ok(entries) = fs::read_dir(cat_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.is_file() {
                                let name = path.file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("unknown")
                                    .to_owned();
                                let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                                files.push(AttachmentFile {
                                    name,
                                    _path: path,
                                    size,
                                    category: cat.to_owned(),
                                });
                            }
                        }
                    }
                }
            }
        }
        let _ = tx.send(GalleryEvent::Finished(Ok(files)));
    });
    
    app.gallery_rx = Some(rx);
}

pub(crate) fn poll_gallery(app: &mut AppState) {
    if let Some(rx) = &app.gallery_rx {
        match rx.try_recv() {
            Ok(GalleryEvent::Finished(result)) => {
                app.gallery_loading = false;
                if let Ok(files) = result {
                    app.gallery.files = files;
                    app.status = format!("Gallery loaded with {} files.", app.gallery.files.len());
                }
                app.gallery_rx = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                app.gallery_loading = false;
                app.gallery_rx = None;
            }
        }
    }
}

pub(crate) fn filtered_gallery_files(app: &AppState) -> Vec<AttachmentFile> {
    if let Some(cat) = &app.gallery.category_filter {
        app.gallery.files
            .iter()
            .filter(|f| f.category == *cat)
            .cloned()
            .collect()
    } else {
        app.gallery.files.clone()
    }
}

pub(crate) fn switch_gallery_filter(app: &mut AppState, category: Option<String>) {
    app.gallery.category_filter = category;
    app.gallery.cursor = 0;
    app.gallery.scroll = 0;
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
        app.open_message_lines =
            data::load_message_preview(&channel, app.settings.preview_messages)?;
        app.open_channel = Some(channel);
        app.open_message_scroll = 0;
        app.screen = Screen::MessageView;
    }

    Ok(())
}

pub(crate) fn key_help(screen: Screen) -> &'static str {
    match screen {
        Screen::Setup => "type/paste value  [Enter] Next  [Left/Up] Back  [Esc] Quit",
        Screen::Home => {
            "[W/S / ↑↓] Select  [Enter] Open  [1-9] Quick choose  [Tab] Next Tab  [Q] Quit"
        }
        Screen::Overview => "[R] Refresh data  [Tab] Next Tab  [B / Esc] Back to menu  [Q] Quit",
        Screen::SupportActivity => {
            "[↑↓] Choose ticket  [Enter] View detail  [R] Reload  [Tab] Next Tab  [B / Esc] Back"
        }
        Screen::SupportTicketDetail => "[↑↓ / K / J] Scroll  [PgUp/Dn] Page  [B / Esc] Done",
        Screen::Activity => {
            "[↑↓] Browse  [Enter] Details  [/ Search] [T/Y/[] Filter  [O] Sort  [C] Clear  [R] Refresh  [B] Back"
        }
        Screen::ActivityDetail => "[↑↓ / K / J] Scroll  [PgUp/Dn] Page  [B / Esc] Done",
        Screen::ChannelList => {
            "[W/S / ↑↓] Navigate  [1-5] Filter type  [Enter] View messages  [B / Esc] Back"
        }
        Screen::MessageView => "[↑↓ / K / J] Scroll content  [PgUp/Dn] Page  [B / Esc] Back to list",
        Screen::Gallery => {
            "[↑↓] Select  [1-9] Category Filter  [B / Esc] Back to menu"
        }
        Screen::Settings => {
            "[W/S / ↑↓] Select  [←→] Adjust  [Enter] Toggle/Apply  [B / Esc] Back"
        }
        Screen::Analyzing | Screen::Downloading => "Current operation in progress... [Please Wait]",
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
    // Only load if we don't have data yet. 
    // Navigation should not trigger a disk sync by default.
    // If the user wants a refresh, they have the 'R' key.
    if app.last_data.is_some() {
        return;
    }

    let results_dir = app.config.results_path(&app.config_path, &app.id);
    let data_path = results_dir.join("data.json");
    if !data_path.exists() {
        return;
    }

    let start = Instant::now();
    log_msg("Starting lazy data load...");
    
    if let Ok(data) = analyzer::read_data(&results_dir) {
        let elapsed = start.elapsed();
        log_msg(&format!("Data loaded in {:?}", elapsed));
        app.last_data = data;
        
        let mtime = fs::metadata(&data_path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        app.last_data_mtime = mtime;
    } else {
        log_msg("Data load FAILED");
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
        let _ = tx.send(AnalysisEvent::Finished(Box::new(result)));
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
                Ok(AnalysisEvent::Finished(result)) => match *result {
                    Ok(data) => {
                        app.analysis_running = false;
                        app.analysis_started_at = None;
                        app.analysis_progress = 1.0;
                        app.status = "Analysis finished successfully.".to_owned();
                        app.error = None;

                        let links = data.messages.attachment_links.clone();
                        app.last_data = Some(data);
                        app.last_data_mtime = SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .ok()
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);
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
                    Err(err) => {
                        app.analysis_running = false;
                        app.analysis_started_at = None;
                        app.analysis_progress = 0.0;
                        app.error = Some(err);
                        app.status = "Analysis failed.".to_owned();
                        app.screen = Screen::Home;
                        finished = true;
                        break;
                    }
                },
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
    
    // Check if we already have the counts in our analysis data
    if let Some(data) = &app.last_data {
        let mut channels = Vec::new();
        for (id, cached) in &data.channels_cache {
            let channel_dir = package_dir.join("messages").join(id);
            if !channel_dir.is_dir() {
                continue;
            }

            channels.push(MessageChannel {
                id: id.clone(),
                title: cached.channel_title.clone(),
                kind: data::detect_channel_kind_str(&cached.channel_type),
                message_count: cached.message_count as usize,
                messages_path: channel_dir.join("messages.json"),
            });
        }

        if !channels.is_empty() {
            channels.sort_by(|a, b| {
                b.message_count
                    .cmp(&a.message_count)
                    .then_with(|| a.title.cmp(&b.title))
            });
            app.channel_cache = Some(channels);
            app.status = "Channel index loaded from cache".to_owned();
            return Ok(());
        }
    }

    // Fallback to disk scanning if no analysis data or empty
    let channels = data::load_channels(&package_dir, &app.config.source_aliases)?;

    app.channel_cache = Some(channels);
    app.status = if app
        .channel_cache
        .as_ref()
        .is_some_and(|channels| channels.is_empty())
    {
        "Messages directory not found or empty".to_owned()
    } else {
        "Channel index loaded from disk".to_owned()
    };

    Ok(())
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

use crate::data::utils::parse_date_key as normalize_date_filter;
