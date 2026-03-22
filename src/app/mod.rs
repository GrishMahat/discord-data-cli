// The brain center of the operation. Keeps track of everything so you don't have to.

use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool, mpsc::Receiver},
    time::Instant,
    fs,
    env,
};
use serde::{Deserialize, Serialize};
use anyhow::{Context, Result};

use crate::{analyzer, config::AppConfig, data};
pub(crate) use data::{ActivityEventPreview, SupportTicketView, ChannelKind};

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
            download_attachments: false,  // By default, don't download that embarrassing video
            preview_messages: 40,          // Show 40 messages of shame per channel
        }
    }
}

// Which channels do you want to relive? ALL OF THEM? Bold choice.
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
            ChannelFilter::Dm => "DMs",  // Where you said things you'd never say in public
            ChannelFilter::GroupDm => "Group DMs",
            ChannelFilter::PublicThread => "Public Threads",  // Arguments for everyone to enjoy
            ChannelFilter::Voice => "Voice",  // Your sleep-deprived ramblings
        }
    }
}

// Every state this app can be in. It's like a Tamagotchi, but for analyzing your life choices.
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
    Newest,     // What have you done recently?
    Oldest,     // The good old days... or were they?
    EventType,  // Let's group the cringe by category!
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
        // First date with the app: show us where your Discord lives.
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
    pub(crate) download_progress: f32,
    pub(crate) download_running: bool,
    pub(crate) download_rx: Option<Receiver<DownloadEvent>>,
    pub(crate) screen: Screen,
    pub(crate) should_quit: bool,
    pub(crate) animation_tick: u64,
    pub(crate) home_cursor: usize,
    pub(crate) settings_cursor: usize,
    pub(crate) channel_cursor: usize,
    pub(crate) current_filter: ChannelFilter,
    pub(crate) open_channel: Option<data::MessageChannel>,
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
    pub(crate) _last_data_mtime: u64,
    pub(crate) support_activity_loading: bool,
    pub(crate) activity_loading: bool,
    pub(crate) support_activity_rx: Option<Receiver<SupportActivityEvent>>,
    pub(crate) gallery_loading: bool,
    pub(crate) gallery_rx: Option<Receiver<GalleryEvent>>,
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

        let cwd = env::current_dir().with_context(|| "failed to read current directory".to_owned())?;
        let default_export = cwd.display().to_string();

        // Spawn a new app with default values. It's like a baby, but made of code.
        let mut app = Self {
            config: session.as_ref().map(|s| s.config.clone()).unwrap_or_default(),
            config_path: config_path.clone(),
            id: session.as_ref().map(|s| s.id.clone()).unwrap_or_default(),
            setup: SetupState::new(default_export),
            settings: session.as_ref().map(|s| s.settings.clone()).unwrap_or_default(),
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
            _last_data_mtime: 0,
            support_activity_loading: false,
            activity_loading: false,
            support_activity_rx: None,
            gallery_loading: false,
            gallery_rx: None,
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
}

// The main menu. All your life choices, neatly organized.
pub(crate) const HOME_MENU_ITEMS: [(&str, &str); 13] = [
    ("Analyze Now", "Run full analysis on your Discord export"),
    ("Overview", "View analysis summary and statistics"),
    ("Support Tickets", "Browse support tickets with details"),
    ("Activity Explorer", "Browse detailed activity with advanced filters and sorting"),
    ("Download Attachments", "Download media files from your messages"),
    ("Gallery", "Browse and search through downloaded attachments"),
    ("Messages (All)", "Browse all message channels"),
    ("DMs", "Browse direct message channels"),
    ("Group DMs", "Browse group direct messages"),
    ("Public Threads", "Browse public thread channels"),
    ("Voice Channels", "Browse voice channel logs"),
    ("Settings", "Configure analyzer options"),
    ("Quit", "Exit the application"),
];
