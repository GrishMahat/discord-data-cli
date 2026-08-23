// The brain center of the operation. Keeps track of everything so you don't have to.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool, mpsc::Receiver},
    time::Instant,
};

use crate::{analyzer, config::AppConfig, data};
pub(crate) use data::{
    ActivityEventPreview, ChannelKind, MessageChannel, SupportTicketView,
};

pub(crate) mod events;
pub(crate) mod state;

pub(crate) use events::*;
pub(crate) use state::*;

// Settings you didn't ask for but got anyway. You're welcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct InteractiveSettings {
    pub(crate) download_attachments: bool,
    pub(crate) preview_messages: usize,
}

impl Default for InteractiveSettings {
    fn default() -> Self {
        Self {
            download_attachments: false, // By default, don't download that embarrassing video
            preview_messages: 40,        // Show 40 messages of shame per channel
        }
    }
}

// Which channels do you want to relive? ALL OF THEM? Bold choice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ChannelFilter {
    #[default]
    All,
    Dm,
    GroupDm,
    PublicThread,
    Voice,
}

/// Sort order for the channel browser list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ChannelSortMode {
    /// Most messages first (default).
    #[default]
    Count,
    /// A -> Z by title.
    Name,
    /// Most recently active first (falls back to count when unknown).
    Recent,
}

impl ChannelSortMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            ChannelSortMode::Count => "most msgs",
            ChannelSortMode::Name => "A-Z",
            ChannelSortMode::Recent => "recent",
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            ChannelSortMode::Count => ChannelSortMode::Name,
            ChannelSortMode::Name => ChannelSortMode::Recent,
            ChannelSortMode::Recent => ChannelSortMode::Count,
        }
    }
}

impl ChannelFilter {
    pub(crate) fn label(self) -> &'static str {
        match self {
            ChannelFilter::All => "All",
            ChannelFilter::Dm => "DMs", // Where you said things you'd never say in public
            ChannelFilter::GroupDm => "Group DMs",
            ChannelFilter::PublicThread => "Public Threads", // Arguments for everyone to enjoy
            ChannelFilter::Voice => "Voice",                 // Your sleep-deprived ramblings
        }
    }
}

// Every state this app can be in. It's like a Tamagotchi, but for analyzing your life choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Screen {
    Setup,
    Home,
    Overview,
    Insights,
    Compare,
    ActivityMap,
    SupportActivity,
    SupportTicketDetail,
    Activity,
    ActivityDetail,
    ChannelList,
    MessageView,
    Search,
    Settings,
    Analyzing,
    Downloading,
    Gallery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SupportActivityTab {
    Support,
    Activity,
    Search,
}

impl SupportActivityTab {
    pub(crate) fn label(self) -> &'static str {
        match self {
            SupportActivityTab::Support => "Support Tickets",
            SupportActivityTab::Activity => "Activity Events",
            SupportActivityTab::Search => "Search",
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            SupportActivityTab::Support => SupportActivityTab::Activity,
            SupportActivityTab::Activity => SupportActivityTab::Search,
            SupportActivityTab::Search => SupportActivityTab::Support,
        }
    }

    pub(crate) fn prev(self) -> Self {
        match self {
            SupportActivityTab::Support => SupportActivityTab::Search,
            SupportActivityTab::Activity => SupportActivityTab::Support,
            SupportActivityTab::Search => SupportActivityTab::Activity,
        }
    }
}

// For when you need to find THAT ONE MESSAGE from THREE YEARS AGO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActivityFilterField {
    Query,
    EventType,
    SourceFile,
    FromDate,
    ToDate,
}

// In what order should your digital archaeology be presented?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActivitySortMode {
    Newest,    // What have you done recently?
    Oldest,    // The good old days... or were they?
    EventType, // Let's group the cringe by category!
}

impl ActivitySortMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            ActivitySortMode::Newest => "newest",
            ActivitySortMode::Oldest => "oldest",
            ActivitySortMode::EventType => "type",
        }
    }

    // Cycle through modes. It's like a slot machine but with less money.
    pub(crate) fn next(self) -> Self {
        match self {
            ActivitySortMode::Newest => ActivitySortMode::Oldest,
            ActivitySortMode::Oldest => ActivitySortMode::EventType,
            ActivitySortMode::EventType => ActivitySortMode::Newest,
        }
    }
}

// The filters that stand between you and your message history.
// Don't worry, we'll find that message from 2019. Eventually.
#[derive(Debug, Clone, Default)]
pub(crate) struct ActivityFilters {
    pub(crate) query: String,
    pub(crate) event_type: String,
    pub(crate) source_file: String,
    pub(crate) from_date: String,
    pub(crate) to_date: String,
}

// The steps to happiness (or at least to seeing your data).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetupStep {
    ExportPath,
    ResultsPath,
    ProfileId,
    Confirm,
}

// A single entry visible in the folder browser panel.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct BrowseEntry {
    pub(crate) name: String,
    pub(crate) path: std::path::PathBuf,
    pub(crate) is_dir: bool,
    /// True if this folder looks like a Discord data export.
    pub(crate) is_discord_export: bool,
}

/// Real-time validation result for the current input path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PathValidation {
    /// Path is empty
    Empty,
    /// Path exists and is a directory
    ValidDir,
    /// Path is a valid Discord export directory
    ValidExport { folders: Vec<String> },
    /// Path doesn't exist
    NotFound,
    /// Path exists but is a file, not a directory
    NotADir,
}

#[derive(Debug, Clone)]
pub(crate) struct SetupState {
    pub(crate) step: SetupStep,
    pub(crate) input: String,
    pub(crate) export_path: String,
    pub(crate) results_path: String,
    pub(crate) profile_id: String,
    pub(crate) notice: String,
    /// Entries shown in the inline folder browser.
    pub(crate) browse_entries: Vec<BrowseEntry>,
    /// Currently highlighted entry in the folder browser.
    pub(crate) browse_cursor: usize,
    /// Whether focus is on the browse panel (true) or the text input (false).
    pub(crate) browse_focus: bool,
    /// Real-time validation of the current input path.
    pub(crate) path_validation: PathValidation,
    /// Scroll offset for browse list (for long lists).
    pub(crate) browse_scroll: usize,
}

impl SetupState {
    fn new(default_export: String) -> Self {
        let entries = list_browse_entries(&default_export);
        let validation = validate_path(&default_export);
        Self {
            step: SetupStep::ExportPath,
            input: default_export.clone(),
            export_path: default_export,
            results_path: String::new(),
            profile_id: String::new(),
            notice: String::new(),
            browse_entries: entries,
            browse_cursor: 0,
            browse_focus: false,
            path_validation: validation,
            browse_scroll: 0,
        }
    }
}

/// Validate a path and detect whether it's a Discord export.
pub(crate) fn validate_path(path: &str) -> PathValidation {
    use std::path::PathBuf;

    let trimmed = path.trim();
    if trimmed.is_empty() {
        return PathValidation::Empty;
    }

    let p = PathBuf::from(trimmed);
    if !p.exists() {
        return PathValidation::NotFound;
    }
    if !p.is_dir() {
        return PathValidation::NotADir;
    }

    // Check for Discord export markers
    let discord_folders = detect_discord_folders(&p);
    if discord_folders.is_empty() {
        PathValidation::ValidDir
    } else {
        PathValidation::ValidExport {
            folders: discord_folders,
        }
    }
}

/// Check if a directory looks like a Discord data export.
/// Returns the list of recognized subfolders found.
fn detect_discord_folders(dir: &std::path::Path) -> Vec<String> {
    let known = [
        "messages",
        "servers",
        "activity",
        "account",
        "programs",
        "README.txt",
    ];
    let mut found = Vec::new();
    for name in known {
        if dir.join(name).exists() {
            found.push(name.to_owned());
        }
    }
    found
}

/// List subdirectories (and parent) of the directory at `path` for the browse panel.
/// Marks directories that look like Discord exports.
pub(crate) fn list_browse_entries(path: &str) -> Vec<BrowseEntry> {
    use std::path::PathBuf;

    let dir = PathBuf::from(path.trim());
    let target = if dir.is_dir() {
        dir.clone()
    } else if let Some(parent) = dir.parent() {
        if parent.is_dir() {
            parent.to_path_buf()
        } else {
            return Vec::new();
        }
    } else {
        return Vec::new();
    };

    let mut entries = Vec::new();

    // Parent directory entry (".. (up)")
    if let Some(parent) = target.parent() {
        entries.push(BrowseEntry {
            name: "⬆  ..".to_owned(),
            path: parent.to_path_buf(),
            is_dir: true,
            is_discord_export: false,
        });
    }

    // Read directory contents — directories only, sorted with Discord exports first
    if let Ok(read) = std::fs::read_dir(&target) {
        let mut dirs: Vec<_> = read
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .map(|n| !n.starts_with('.'))
                    .unwrap_or(false)
            })
            .collect();
        dirs.sort_by_key(|e| e.file_name());

        // Separate Discord exports from regular folders (exports come first)
        let mut export_entries = Vec::new();
        let mut regular_entries = Vec::new();

        for entry in dirs.iter().take(30) {
            let name = entry.file_name().to_str().unwrap_or("?").to_owned();
            let is_export = !detect_discord_folders(&entry.path()).is_empty();
            let display_name = if is_export {
                format!("⭐ {name}")
            } else {
                format!("📁 {name}")
            };
            let browse = BrowseEntry {
                name: display_name,
                path: entry.path(),
                is_dir: true,
                is_discord_export: is_export,
            };
            if is_export {
                export_entries.push(browse);
            } else {
                regular_entries.push(browse);
            }
        }

        entries.extend(export_entries);
        entries.extend(regular_entries);
    }

    entries
}

// What the config file looks like on disk. Spoiler: it's TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct InteractiveSession {
    pub(crate) config: AppConfig,
    pub(crate) id: String,
    pub(crate) settings: InteractiveSettings,
}

// A file in the gallery. Could be an image of your lunch. Could be something else.
#[derive(Debug, Clone)]
pub(crate) struct AttachmentFile {
    pub(crate) name: String,
    pub(crate) _path: PathBuf,
    pub(crate) size: u64,
    pub(crate) category: String,
}

// The gallery state. It remembers where you were, unlike you remembering where you put that file.
#[derive(Debug, Clone)]
pub(crate) struct GalleryState {
    pub(crate) files: Vec<AttachmentFile>,
    pub(crate) cursor: usize,
    pub(crate) scroll: usize,
    pub(crate) category_filter: Option<String>,
}

/// Which part of the search screen has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SearchField {
    #[default]
    Input,
    TypeFilter,
    HasFilter,
    Results,
}

/// Content filter for message search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SearchHasFilter {
    #[default]
    Any,
    Attachments,
    Links,
}

impl SearchHasFilter {
    pub(crate) fn label(self) -> &'static str {
        match self {
            SearchHasFilter::Any => "Any",
            SearchHasFilter::Attachments => "Attachments",
            SearchHasFilter::Links => "Links",
        }
    }
}

/// A single search hit: one message inside one channel.
#[derive(Debug, Clone)]
pub(crate) struct SearchResult {
    /// Folder name under messages/ — used to reopen the channel context.
    pub(crate) channel_key: String,
    pub(crate) title: String,
    pub(crate) kind: data::ChannelKind,
    pub(crate) timestamp: String,
    pub(crate) content: String,
}

impl Default for SearchResult {
    fn default() -> Self {
        Self {
            channel_key: String::new(),
            title: String::new(),
            kind: data::ChannelKind::Other,
            timestamp: String::new(),
            content: String::new(),
        }
    }
}

/// State for the global message search screen.
#[derive(Default)]
pub(crate) struct SearchState {
    pub(crate) input: String,
    pub(crate) field: SearchField,
    pub(crate) type_filter: ChannelFilter,
    pub(crate) has_filter: SearchHasFilter,
    pub(crate) results: Vec<SearchResult>,
    pub(crate) cursor: usize,
    pub(crate) scroll: usize,
    pub(crate) running: bool,
    pub(crate) scanned_files: usize,
    pub(crate) total_files: usize,
    pub(crate) total_matches: usize,
    pub(crate) truncated: bool,
    /// Bumped on every new run; stale worker events are dropped by generation.
    pub(crate) generation: u64,
    pub(crate) rx: Option<std::sync::mpsc::Receiver<crate::app::SearchEvent>>,
    pub(crate) cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

// THE ONE RING TO RULE THEM ALL. Everything this app knows lives here.
pub(crate) struct AppState {
    pub(crate) config: AppConfig,
    pub(crate) config_path: PathBuf,
    pub(crate) id: String,
    pub(crate) setup: SetupState,
    pub(crate) settings: InteractiveSettings,
    pub(crate) channel_cache: Option<Vec<data::MessageChannel>>,
    pub(crate) last_data: Option<analyzer::AnalysisData>,
    pub(crate) status: String,
    pub(crate) error: Option<String>,
    pub(crate) analysis_progress: f32,
    pub(crate) analysis_step: analyzer::AnalysisStep,
    pub(crate) analysis_running: bool,
    pub(crate) analysis_abort: Arc<AtomicBool>,
    pub(crate) analysis_started_at: Option<Instant>,
    pub(crate) analysis_rx: Option<Receiver<AnalysisEvent>>,
    pub(crate) analysis_current_file: Option<String>,
    pub(crate) analysis_files_processed: Option<u32>,
    pub(crate) analysis_total_files: Option<u32>,
    pub(crate) download_progress: f32,
    pub(crate) download_running: bool,
    pub(crate) download_abort: Arc<AtomicBool>,
    pub(crate) download_rx: Option<Receiver<DownloadEvent>>,
    pub(crate) screen: Screen,
    pub(crate) should_quit: bool,
    pub(crate) animation_tick: u64,
    pub(crate) sidebar_cursor: Option<usize>,
    pub(crate) home_cursor: usize,
    pub(crate) settings_cursor: usize,
    pub(crate) channel_cursor: usize,
    pub(crate) current_filter: ChannelFilter,
    pub(crate) channel_sort: ChannelSortMode,
    pub(crate) open_channel: Option<data::MessageChannel>,
    pub(crate) open_message_lines: Vec<String>,
    pub(crate) open_message_scroll: usize,
    pub(crate) support_tickets: Option<Vec<SupportTicketView>>,
    pub(crate) support_ticket_cursor: usize,
    pub(crate) support_ticket_scroll: usize,
    pub(crate) support_activity_tab: SupportActivityTab,
    pub(crate) activity_events: Option<Vec<ActivityEventPreview>>,
    pub(crate) activity_cursor: usize,
    pub(crate) activity_filters: ActivityFilters,
    pub(crate) activity_filter_edit: Option<ActivityFilterField>,
    pub(crate) activity_sort: ActivitySortMode,
    pub(crate) activity_detail_scroll: usize,
    pub(crate) gallery: GalleryState,
    pub(crate) _last_data_mtime: u64,
    pub(crate) support_activity_loading: bool,
    pub(crate) activity_loading: bool,
    pub(crate) support_tickets_rx: Option<Receiver<SupportActivityEvent>>,
    pub(crate) activity_events_rx: Option<Receiver<SupportActivityEvent>>,
    /// Set when the last ticket load failed; blocks auto-respawn loops.
    pub(crate) support_tickets_failed: Option<String>,
    /// Set when the last activity load failed; blocks auto-respawn loops.
    pub(crate) activity_failed: Option<String>,
    pub(crate) channel_loading: bool,
    pub(crate) channel_rx: Option<Receiver<ChannelEvent>>,
    pub(crate) gallery_loading: bool,
    pub(crate) gallery_rx: Option<Receiver<GalleryEvent>>,
    /// Live message preview shown in the channel browser split pane.
    pub(crate) channel_preview_lines: Vec<String>,
    pub(crate) channel_preview_for: Option<String>,
    pub(crate) channel_preview_scroll: usize,
    pub(crate) channel_preview_loading: bool,
    pub(crate) channel_preview_rx: Option<Receiver<ChannelPreviewEvent>>,
    pub(crate) search: SearchState,
    pub(crate) insights_scroll: usize,
    pub(crate) compare_cursor: usize,
    /// Year offset (from newest) selected in the activity heatmap.
    pub(crate) map_page: usize,
}

impl AppState {
    pub(crate) fn new(config_path: PathBuf) -> Result<Self> {
        // Check if we have a previous session. We remember, even when you don't.
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

        // Spawn a new app with default values. It's like a baby, but made of code.
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
            analysis_step: analyzer::AnalysisStep::Preparing,
            analysis_running: false,
            analysis_abort: Arc::new(AtomicBool::new(false)),
            analysis_started_at: None,
            analysis_rx: None,
            analysis_current_file: None,
            analysis_files_processed: None,
            analysis_total_files: None,
            download_progress: 0.0,
            download_running: false,
            download_abort: Arc::new(AtomicBool::new(false)),
            download_rx: None,
            screen: Screen::Setup,
            should_quit: false,
            animation_tick: 0,
            sidebar_cursor: None,
            home_cursor: 0,
            settings_cursor: 0,
            channel_cursor: 0,
            current_filter: ChannelFilter::All,
            channel_sort: ChannelSortMode::Count,
            open_channel: None,
            open_message_lines: Vec::new(),
            open_message_scroll: 0,
            support_tickets: None,
            support_ticket_cursor: 0,
            support_ticket_scroll: 0,
            support_activity_tab: SupportActivityTab::Support,
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
            _last_data_mtime: 0,
            support_activity_loading: false,
            activity_loading: false,
            support_tickets_rx: None,
            activity_events_rx: None,
            support_tickets_failed: None,
            activity_failed: None,
            channel_loading: false,
            channel_rx: None,
            gallery_loading: false,
            gallery_rx: None,
            channel_preview_lines: Vec::new(),
            channel_preview_for: None,
            channel_preview_scroll: 0,
            channel_preview_loading: false,
            channel_preview_rx: None,
            search: SearchState::default(),
            insights_scroll: 0,
            compare_cursor: 0,
            map_page: 0,
        };

        // If there was a session, pick up where we left off!
        if session.is_some() {
            let pkg_dir = app.config.package_path(&app.config_path, &app.id);
            if pkg_dir.exists() {
                app.screen = Screen::Home;
                state::try_load_existing_data(&mut app);
                app.status = "Session loaded. Ready.".to_owned();
            }
        }

        Ok(app)
    }

    // "Save your progress" - the game's way of being responsible.
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

    /// Snapshots on disk for the Compare screen, newest first.
    pub(crate) fn compare_snapshots(&self) -> Vec<(String, crate::compare::Snapshot)> {
        let results_dir = self.config.results_path(&self.config_path, &self.id);
        crate::compare::list_snapshots(&results_dir)
    }
}

/// Minimal AppState for UI unit tests (no disk access).
#[cfg(test)]
pub(crate) fn test_app_with_data(data: analyzer::AnalysisData) -> AppState {
    let mut app = AppState {
        config: AppConfig::default(),
        config_path: PathBuf::from("/tmp/opencode/none.toml"),
        id: String::new(),
        setup: SetupState::new(String::new()),
        settings: InteractiveSettings::default(),
        channel_cache: None,
        last_data: Some(data),
        status: String::new(),
        error: None,
        analysis_progress: 0.0,
        analysis_step: analyzer::AnalysisStep::Complete,
        analysis_running: false,
        analysis_abort: Arc::new(AtomicBool::new(false)),
        analysis_started_at: None,
        analysis_rx: None,
        analysis_current_file: None,
        analysis_files_processed: None,
        analysis_total_files: None,
        download_progress: 0.0,
        download_running: false,
        download_abort: Arc::new(AtomicBool::new(false)),
        download_rx: None,
        screen: Screen::Overview,
        should_quit: false,
        animation_tick: 0,
        sidebar_cursor: None,
        home_cursor: 0,
        settings_cursor: 0,
        channel_cursor: 0,
        current_filter: ChannelFilter::All,
        channel_sort: ChannelSortMode::Count,
        open_channel: None,
        open_message_lines: Vec::new(),
        open_message_scroll: 0,
        support_tickets: None,
        support_ticket_cursor: 0,
        support_ticket_scroll: 0,
        support_activity_tab: SupportActivityTab::Support,
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
        _last_data_mtime: 0,
        support_activity_loading: false,
        activity_loading: false,
        support_tickets_rx: None,
        activity_events_rx: None,
        support_tickets_failed: None,
        activity_failed: None,
        channel_loading: false,
        channel_rx: None,
        gallery_loading: false,
        gallery_rx: None,
        channel_preview_lines: Vec::new(),
        channel_preview_for: None,
        channel_preview_scroll: 0,
        channel_preview_loading: false,
        channel_preview_rx: None,
        search: SearchState::default(),
        insights_scroll: 0,
        compare_cursor: 0,
        map_page: 0,
    };
    app.screen = Screen::ActivityMap;
    app
}

// The main menu. All your life choices, neatly organized.
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
    (
        "Gallery",
        "Browse and search through downloaded attachments",
    ),
    ("Messages (All)", "Browse all message channels"),
    ("DMs", "Browse direct message channels"),
    ("Group DMs", "Browse group direct messages"),
    ("Public Threads", "Browse public thread channels"),
    ("Voice Channels", "Browse voice channel logs"),
    ("Settings", "Configure analyzer options"),
    ("Quit", "Exit the application"),
];
