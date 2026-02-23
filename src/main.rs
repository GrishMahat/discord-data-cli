mod analyzer;
mod config;
mod downloader;

use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    io::{self, BufReader, IsTerminal},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use config::AppConfig;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Alignment, Color, Modifier, Position, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Wrap},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InteractiveSettings {
    download_attachments: bool,
    preview_messages: usize,
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
struct MessageChannel {
    id: String,
    title: String,
    kind: ChannelKind,
    message_count: usize,
    messages_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChannelKind {
    Dm,
    GroupDm,
    PublicThread,
    Voice,
    Guild,
    Other,
}

impl ChannelKind {
    fn label(self) -> &'static str {
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
enum ChannelFilter {
    All,
    Dm,
    GroupDm,
    PublicThread,
    Voice,
}

impl ChannelFilter {
    fn label(self) -> &'static str {
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
enum Screen {
    Setup,
    Home,
    Overview,
    ChannelList,
    MessageView,
    Settings,
    Analyzing,
    Downloading,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupStep {
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
struct SetupState {
    step: SetupStep,
    input: String,
    export_path: String,
    results_path: String,
    profile_id: String,
    notice: String,
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

struct AppState {
    config: AppConfig,
    config_path: PathBuf,
    id: String,
    setup: SetupState,
    settings: InteractiveSettings,
    channel_cache: Option<Vec<MessageChannel>>,
    last_data: Option<analyzer::AnalysisData>,
    status: String,
    error: Option<String>,
    analysis_progress: f32,
    analysis_running: bool,
    analysis_started_at: Option<Instant>,
    analysis_rx: Option<Receiver<AnalysisEvent>>,
    download_progress: f32,
    download_running: bool,
    download_rx: Option<Receiver<DownloadEvent>>,
    screen: Screen,
    should_quit: bool,
    animation_tick: u64,
    home_cursor: usize,
    settings_cursor: usize,
    channel_cursor: usize,
    current_filter: ChannelFilter,
    open_channel: Option<MessageChannel>,
    open_message_lines: Vec<String>,
    open_message_scroll: usize,
}

impl AppState {
    fn new(config_path: PathBuf) -> Result<Self> {
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
        };

        if session.is_some() {
            let pkg_dir = app.config.package_path(&app.config_path, &app.id);
            let _res_dir = app.config.results_path(&app.config_path, &app.id);
            if pkg_dir.exists() {
                app.screen = Screen::Home;
                try_load_existing_data(&mut app);
                app.status = "Session loaded. Ready.".to_owned();
            }
        }

        Ok(app)
    }

    fn save_session(&self) {
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

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().with_context(|| "failed to enable raw mode".to_owned())?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, cursor::Hide)
            .with_context(|| "failed to enter alternate screen".to_owned())?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, cursor::Show, LeaveAlternateScreen);
    }
}

fn main() -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("Discord Analyzer TTY requires an interactive terminal session.");
    }

    let config_path = env::current_dir()
        .with_context(|| "failed to read current working directory".to_owned())?
        .join("interactive.session.toml");

    let mut app = AppState::new(config_path)?;
    run_tui(&mut app)
}

fn run_tui(app: &mut AppState) -> Result<()> {
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal =
        Terminal::new(backend).with_context(|| "failed to create terminal".to_owned())?;
    terminal
        .clear()
        .with_context(|| "failed to clear terminal".to_owned())?;

    while !app.should_quit {
        app.animation_tick = app.animation_tick.wrapping_add(1);
        poll_analysis(app);
        poll_download(app);

        terminal
            .draw(|frame| draw_ui(frame, app))
            .with_context(|| "failed to draw frame".to_owned())?;

        if event::poll(Duration::from_millis(50)).with_context(|| "event poll failed".to_owned())? {
            match event::read().with_context(|| "event read failed".to_owned())? {
                Event::Key(key) if key.kind == KeyEventKind::Press => handle_key(app, key)?,
                Event::Paste(text) => handle_paste(app, &text),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }

    Ok(())
}

fn draw_ui(frame: &mut ratatui::Frame<'_>, app: &AppState) {
    if app.screen == Screen::Setup {
        draw_setup(frame, app);
        return;
    }

    if app.screen == Screen::Analyzing {
        draw_analyzing(frame, app);
        return;
    }

    if app.screen == Screen::Downloading {
        draw_downloading(frame, app);
        return;
    }

    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, app, chunks[0]);
    draw_tabs(frame, app, chunks[1]);

    match app.screen {
        Screen::Home => draw_home(frame, app, chunks[2]),
        Screen::Overview => draw_overview(frame, app, chunks[2]),
        Screen::ChannelList => draw_channels(frame, app, chunks[2]),
        Screen::MessageView => draw_message_view(frame, app, chunks[2]),
        Screen::Settings => draw_settings(frame, app, chunks[2]),
        _ => {}
    }

    draw_statusbar(frame, app, chunks[3]);
}

fn draw_header(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Discord Data Analyzer ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split header into left (user/path) and right (quick stats)
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(40), Constraint::Length(40)])
        .split(inner);

    let user_str = if let Some(data) = &app.last_data {
        format!(
            "  {}  ({})",
            data.account.username.as_deref().unwrap_or("unknown"),
            data.account.user_id.as_deref().unwrap_or("?")
        )
    } else {
        "  Not analyzed yet".to_owned()
    };

    let status_color = if app.error.is_some() {
        Color::Red
    } else if app.status.contains("complete") || app.status.contains("Ready") || app.status.contains("loaded") {
        Color::Green
    } else {
        Color::Yellow
    };

    let left_lines = vec![
        Line::styled(user_str, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Line::styled(
            format!("  {}", app.error.as_deref().unwrap_or(app.status.as_str())),
            Style::default().fg(status_color),
        ),
    ];
    frame.render_widget(Paragraph::new(left_lines), cols[0]);

    // Quick stats on the right
    if let Some(data) = &app.last_data {
        let right_lines = vec![
            Line::from(vec![
                ratatui::text::Span::styled("msgs ", Style::default().fg(Color::Gray)),
                ratatui::text::Span::styled(
                    format!("{}", data.messages.total),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ),
                ratatui::text::Span::raw("  "),
                ratatui::text::Span::styled("servers ", Style::default().fg(Color::Gray)),
                ratatui::text::Span::styled(
                    format!("{}", data.servers.count),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                ratatui::text::Span::styled("channels ", Style::default().fg(Color::Gray)),
                ratatui::text::Span::styled(
                    format!("{}", data.messages.channels),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ),
                ratatui::text::Span::raw("  "),
                ratatui::text::Span::styled("avg len ", Style::default().fg(Color::Gray)),
                ratatui::text::Span::styled(
                    format!("{:.0}ch", data.messages.content.avg_length_chars),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ),
            ]),
        ];
        frame.render_widget(Paragraph::new(right_lines).alignment(Alignment::Right), cols[1]);
    }
}

fn draw_tabs(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let screens = [
        (Screen::Home,        "Home"),
        (Screen::Overview,    "Overview"),
        (Screen::ChannelList, "Channels"),
        (Screen::Settings,    "Settings"),
    ];

    let mut spans = vec![ratatui::text::Span::raw(" ")];
    for (screen, label) in &screens {
        let active = app.screen == *screen;
        let style = if active {
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(ratatui::text::Span::styled(format!(" {label} "), style));
        spans.push(ratatui::text::Span::raw("  "));
    }

    let tabs = Paragraph::new(Line::from(spans))
        .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(Color::DarkGray)));
    frame.render_widget(tabs, area);
}

fn draw_statusbar(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let help = key_help(app.screen);
    let bar = Paragraph::new(Line::from(vec![
        ratatui::text::Span::styled(" ? ", Style::default().fg(Color::Black).bg(Color::DarkGray)),
        ratatui::text::Span::raw(" "),
        ratatui::text::Span::styled(help, Style::default().fg(Color::DarkGray)),
    ]))
    .block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(Color::DarkGray)));
    frame.render_widget(bar, area);
}

fn draw_analyzing(frame: &mut ratatui::Frame<'_>, app: &AppState) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(Color::Black)), area);
    let card = centered_rect(64, 50, area);
    frame.render_widget(Clear, card);

    let block = Block::default()
        .borders(Borders::ALL)
        .title("  Analyzing Discord Export  ")
        .border_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    let inner = block.inner(card);
    frame.render_widget(block, card);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(3), Constraint::Length(2)])
        .split(inner);

    let spinner_frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let spinner = spinner_frames[((app.animation_tick / 2) % spinner_frames.len() as u64) as usize];
    let elapsed = app
        .analysis_started_at
        .map(|s| s.elapsed())
        .unwrap_or_default();

    let text_lines = vec![
        Line::from(""),
        Line::styled(
            format!(" {spinner} Analyzing your Discord data..."),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::styled(
            format!("    {}", app.status),
            Style::default().fg(Color::Gray),
        ),
    ];
    frame.render_widget(
        Paragraph::new(text_lines).wrap(Wrap { trim: true }),
        sections[0],
    );

    let pct = app.analysis_progress * 100.0;
    let label = format!("  {pct:>5.1}%  elapsed: {}", format_duration(elapsed));
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .ratio(app.analysis_progress as f64)
        .label(label);
    frame.render_widget(gauge, sections[1]);

    let eta_line = if app.analysis_progress > 0.02 {
        let rate = app.analysis_progress as f64 / elapsed.as_secs_f64().max(0.001);
        let remaining = ((1.0 - app.analysis_progress as f64) / rate) as u64;
        Line::styled(
            format!("  ETA ~{}", format_duration(Duration::from_secs(remaining))),
            Style::default().fg(Color::DarkGray),
        )
    } else {
        Line::styled("  Calculating ETA...", Style::default().fg(Color::DarkGray))
    };
    frame.render_widget(Paragraph::new(eta_line), sections[2]);
}

fn draw_downloading(frame: &mut ratatui::Frame<'_>, app: &AppState) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(Color::Black)), area);
    let card = centered_rect(64, 50, area);
    frame.render_widget(Clear, card);

    let block = Block::default()
        .borders(Borders::ALL)
        .title("  Downloading Attachments  ")
        .border_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    let inner = block.inner(card);
    frame.render_widget(block, card);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(3)])
        .split(inner);

    let spinner_frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let spinner = spinner_frames[((app.animation_tick / 2) % spinner_frames.len() as u64) as usize];

    let text_lines = vec![
        Line::from(""),
        Line::styled(
            format!(" {spinner} Downloading media and attachments..."),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::styled(
            format!("    {}", app.status),
            Style::default().fg(Color::Gray),
        ),
    ];
    frame.render_widget(Paragraph::new(text_lines).wrap(Wrap { trim: true }), sections[0]);

    let pct = app.download_progress * 100.0;
    let label = format!("  {pct:>5.1}%");
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(Color::Yellow).bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .ratio(app.download_progress as f64)
        .label(label);
    frame.render_widget(gauge, sections[1]);
}

fn draw_setup(frame: &mut ratatui::Frame<'_>, app: &AppState) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        area,
    );

    let card = centered_rect(72, 80, area);
    frame.render_widget(Clear, card);

    let block = Block::default()
        .borders(Borders::ALL)
        .title("  Setup  ")
        .border_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    let inner = block.inner(card);
    frame.render_widget(block, card);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // title
            Constraint::Length(1), // spacer
            Constraint::Length(4), // step progress
            Constraint::Length(4), // instruction
            Constraint::Length(3), // preview
            Constraint::Length(3), // input
            Constraint::Min(2),    // status
        ])
        .split(inner);

    // Title
    let title = Paragraph::new(Line::styled(
        "Discord Data Analyzer",
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(title, sections[0]);

    // Step dots
    let steps = ["Export Path", "Results Dir", "Profile ID", "Confirm"];
    let current_step = match app.setup.step {
        SetupStep::ExportPath => 0,
        SetupStep::ResultsPath => 1,
        SetupStep::ProfileId => 2,
        SetupStep::Confirm => 3,
    };
    let mut dot_spans = Vec::new();
    for (i, name) in steps.iter().enumerate() {
        let (dot, style) = if i < current_step {
            ("●", Style::default().fg(Color::Green))
        } else if i == current_step {
            ("●", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        } else {
            ("○", Style::default().fg(Color::DarkGray))
        };
        dot_spans.push(ratatui::text::Span::styled(format!("{dot} {name}"), style));
        if i < steps.len() - 1 {
            dot_spans.push(ratatui::text::Span::styled("  ──  ", Style::default().fg(Color::DarkGray)));
        }
    }
    frame.render_widget(
        Paragraph::new(Line::from(dot_spans)).alignment(Alignment::Center),
        sections[2],
    );

    // Instruction
    let instruction = match app.setup.step {
        SetupStep::ExportPath  => "Paste the full path to your extracted Discord data folder.\nThe directory must already exist.",
        SetupStep::ResultsPath => "Where should results be saved?\nPress Enter to accept the default (inside your export folder).",
        SetupStep::ProfileId   => "Optional: enter a profile ID if you have multiple exports.\nLeave empty and press Enter to skip.",
        SetupStep::Confirm     => "Everything looks good.\nPress Enter to continue, or Left/Up to go back and edit.",
    };
    frame.render_widget(
        Paragraph::new(instruction)
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: true }),
        sections[3],
    );

    // Preview of current values
    let preview = Paragraph::new(vec![
        Line::from(vec![
            ratatui::text::Span::styled("Export:  ", Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(&app.setup.export_path, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            ratatui::text::Span::styled("Results: ", Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(&app.setup.results_path, Style::default().fg(Color::White)),
        ]),
    ])
    .block(Block::default().borders(Borders::LEFT).border_style(Style::default().fg(Color::DarkGray)));
    frame.render_widget(preview, sections[4]);

    // Input box
    if app.setup.step == SetupStep::Confirm {
        let confirm = Paragraph::new(Line::styled(
            "  Press Enter to open Home  ",
            Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center);
        frame.render_widget(confirm, sections[5]);
    } else {
        let input_block = Block::default()
            .borders(Borders::ALL)
            .title(" Input ")
            .border_style(Style::default().fg(Color::Cyan));
        let input_area = input_block.inner(sections[5]);
        frame.render_widget(input_block, sections[5]);

        let max_width = input_area.width.saturating_sub(1) as usize;
        let (display, cursor_offset) = fit_input_for_box(&app.setup.input, max_width);
        frame.render_widget(Paragraph::new(display), input_area);
        frame.set_cursor_position(Position::new(
            input_area.x + cursor_offset as u16,
            input_area.y,
        ));
    }

    // Status / error
    let (notice_text, notice_color) = if app.setup.notice.to_lowercase().contains("error")
        || app.setup.notice.to_lowercase().contains("not found")
        || app.setup.notice.to_lowercase().contains("required")
    {
        (format!("  {}", app.setup.notice), Color::Red)
    } else {
        (format!("  {}", app.setup.notice), Color::Yellow)
    };
    frame.render_widget(
        Paragraph::new(notice_text)
            .style(Style::default().fg(notice_color))
            .wrap(Wrap { trim: true }),
        sections[6],
    );
}

fn draw_home(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    const HOME_ITEMS: [(&str, &str); 10] = [
        ("Analyze Now",       "Run full analysis on your Discord export"),
        ("Overview",          "View analysis summary and statistics"),
        ("Download Attachments", "Download media files from your messages"),
        ("Messages (All)",    "Browse all message channels"),
        ("DMs",               "Browse direct message channels"),
        ("Group DMs",         "Browse group direct messages"),
        ("Public Threads",    "Browse public thread channels"),
        ("Voice Channels",    "Browse voice channel logs"),
        ("Settings",          "Configure analyzer options"),
        ("Quit",              "Exit the application"),
    ];

    // Split into menu (left) and sidebar (right)
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(32), Constraint::Length(36)])
        .split(area);

    // Items that require a completed analysis before they can be used
    const LOCKED_INDICES: [usize; 7] = [1, 2, 3, 4, 5, 6, 7];
    let has_data = app.last_data.is_some();

    // Menu list
    let mut items = Vec::with_capacity(HOME_ITEMS.len());
    for (idx, (label, _)) in HOME_ITEMS.iter().enumerate() {
        let key = format!("{}", idx + 1);
        let is_locked = !has_data && LOCKED_INDICES.contains(&idx);
        if is_locked {
            items.push(ListItem::new(Line::from(vec![
                ratatui::text::Span::styled(
                    format!(" {key} "),
                    Style::default().fg(Color::DarkGray),
                ),
                ratatui::text::Span::raw(" "),
                ratatui::text::Span::styled(
                    label.to_string(),
                    Style::default().fg(Color::DarkGray),
                ),
                ratatui::text::Span::styled(
                    "  [locked]",
                    Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM),
                ),
            ])));
        } else {
            items.push(ListItem::new(Line::from(vec![
                ratatui::text::Span::styled(
                    format!(" {key} "),
                    Style::default().fg(Color::DarkGray),
                ),
                ratatui::text::Span::raw(" "),
                ratatui::text::Span::styled(
                    label.to_string(),
                    Style::default().fg(Color::White),
                ),
            ])));
        }
    }

    // Show description of selected item at bottom
    let selected_desc = HOME_ITEMS
        .get(app.home_cursor)
        .map(|(_, d)| *d)
        .unwrap_or("");
    let cursor_is_locked = !has_data && LOCKED_INDICES.contains(&app.home_cursor);
    let (desc_text, desc_color) = if cursor_is_locked {
        (
            format!("  Requires analysis data — run 'Analyze Now' first"),
            Color::Yellow,
        )
    } else {
        (format!("  {selected_desc}"), Color::Gray)
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(3)])
        .split(cols[0]);

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Menu ")
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::DarkGray)
                .bg(if cursor_is_locked { Color::Reset } else { Color::Cyan })
                .add_modifier(if cursor_is_locked { Modifier::empty() } else { Modifier::BOLD }),
        )
        .highlight_symbol("");

    let mut state = ListState::default();
    state.select(Some(app.home_cursor));
    frame.render_stateful_widget(list, rows[0], &mut state);

    // Description bar
    frame.render_widget(
        Paragraph::new(Line::styled(desc_text, Style::default().fg(desc_color)))
            .block(Block::default().borders(Borders::ALL).border_style(
                Style::default().fg(if cursor_is_locked { Color::Yellow } else { Color::DarkGray }),
            )),
        rows[1],
    );

    // Sidebar
    draw_home_sidebar(frame, app, cols[1]);
}

fn draw_home_sidebar(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Quick Stats ")
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(data) = &app.last_data else {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::styled(
                    "  No data loaded yet.",
                    Style::default().fg(Color::DarkGray),
                ),
                Line::from(""),
                Line::styled(
                    "  Run 'Analyze Now' to",
                    Style::default().fg(Color::DarkGray),
                ),
                Line::styled(
                    "  populate this panel.",
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
            inner,
        );
        return;
    };

    let total_emoji = data.messages.content.emoji_unicode + data.messages.content.emoji_custom;

    let mut lines = vec![
        stat_line("Messages",   &fmt_num(data.messages.total)),
        stat_line("Channels",   &fmt_num(data.messages.channels)),
        stat_line("With text",  &format!("{:.1}%", ratio(data.messages.with_content, data.messages.total) * 100.0)),
        stat_line("Avg length", &format!("{:.0} ch", data.messages.content.avg_length_chars)),
        stat_line("Emoji",      &fmt_num(total_emoji)),
        stat_line("Attach.",    &fmt_num(data.messages.with_attachments)),
        Line::from(""),
        stat_line("Servers",    &fmt_num(data.servers.count)),
        stat_line("Tickets",    &fmt_num(data.support_tickets.count)),
        stat_line("Activity",   &fmt_num(data.activity.total_events)),
        Line::from(""),
    ];

    if let (Some(first), Some(last)) = (
        &data.messages.temporal.first_message_date,
        &data.messages.temporal.last_message_date,
    ) {
        lines.push(Line::styled("  History", Style::default().fg(Color::DarkGray)));
        lines.push(Line::styled(format!("  {first}"), Style::default().fg(Color::Gray)));
        lines.push(Line::styled("  →", Style::default().fg(Color::DarkGray)));
        lines.push(Line::styled(format!("  {last}"), Style::default().fg(Color::Gray)));
    }

    // Most active hour mini chart
    if !data.messages.temporal.by_hour.is_empty() {
        if let Some((&peak_hr, &peak_cnt)) = data.messages.temporal.by_hour.iter().max_by_key(|&(_, c)| c) {
            lines.push(Line::from(""));
            lines.push(Line::styled(
                format!("  Peak {:02}:00  {} msgs", peak_hr, fmt_num(peak_cnt)),
                Style::default().fg(Color::Cyan),
            ));
        }
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn draw_overview(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let Some(data) = &app.last_data else {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::styled(
                    "  No analysis data loaded. Run Analyze Now first.",
                    Style::default().fg(Color::DarkGray),
                ),
            ])
            .block(Block::default().borders(Borders::ALL).title(" Overview ").border_style(Style::default().fg(Color::Cyan))),
            area,
        );
        return;
    };

    // Layout: top row (messages | servers/tickets) + bottom row (hour chart | top words | top channels)
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
        .split(area);

    let top_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(rows[0]);

    let bot_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(30), Constraint::Percentage(30)])
        .split(rows[1]);

    // ── Panel 1: Messages ──────────────────────────────────────────────────
    let total_emoji = data.messages.content.emoji_unicode + data.messages.content.emoji_custom;
    let msg_lines = vec![
        stat_line("Total messages",    &fmt_num(data.messages.total)),
        stat_line("Channels",          &fmt_num(data.messages.channels)),
        stat_line("With text",         &format!("{} ({:.1}%)", fmt_num(data.messages.with_content), ratio(data.messages.with_content, data.messages.total) * 100.0)),
        stat_line("With attachments",  &format!("{} ({:.1}%)", fmt_num(data.messages.with_attachments), ratio(data.messages.with_attachments, data.messages.total) * 100.0)),
        stat_line("Avg length",        &format!("{:.1} chars", data.messages.content.avg_length_chars)),
        stat_line("Total chars",       &fmt_num(data.messages.content.total_chars)),
        stat_line("Emoji (unicode)",   &fmt_num(data.messages.content.emoji_unicode)),
        stat_line("Emoji (custom)",    &fmt_num(data.messages.content.emoji_custom)),
        stat_line("Total emoji",       &fmt_num(total_emoji)),
        stat_line("Line breaks",       &fmt_num(data.messages.content.linebreaks)),
        stat_line("Distinct chars",    &fmt_num(data.messages.content.distinct_characters as u64)),
    ];
    frame.render_widget(
        Paragraph::new(msg_lines)
            .block(Block::default().borders(Borders::ALL).title(" Messages ").border_style(Style::default().fg(Color::Cyan)))
            .wrap(Wrap { trim: true }),
        top_cols[0],
    );

    // ── Panel 2: Servers / Tickets / Activity ─────────────────────────────
    let mut right_lines = vec![
        Line::styled(" Servers", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        stat_line("Count",       &fmt_num(data.servers.count)),
        stat_line("Index entries",&fmt_num(data.servers.index_entries)),
        stat_line("Audit logs",  &fmt_num(data.servers.audit_log_entries)),
        Line::from(""),
        Line::styled(" Support Tickets", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        stat_line("Count",    &fmt_num(data.support_tickets.count)),
        stat_line("Comments", &fmt_num(data.support_tickets.comments)),
        Line::from(""),
        Line::styled(" Activity", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        stat_line("Events",       &fmt_num(data.activity.total_events)),
        stat_line("Parse errors", &format!("{} ({:.2}%)", fmt_num(data.activity.parse_errors), ratio(data.activity.parse_errors, data.activity.total_events) * 100.0)),
    ];

    // channel type breakdown
    if !data.messages.by_channel_type.is_empty() {
        right_lines.push(Line::from(""));
        right_lines.push(Line::styled(" Channel Types", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)));
        for (name, count) in top_counts(&data.messages.by_channel_type, 5) {
            right_lines.push(stat_line(&name, &fmt_num(count)));
        }
    }

    frame.render_widget(
        Paragraph::new(right_lines)
            .block(Block::default().borders(Borders::ALL).title(" Servers & Activity ").border_style(Style::default().fg(Color::Cyan)))
            .wrap(Wrap { trim: true }),
        top_cols[1],
    );

    // ── Panel 3: Hour-of-day bar chart ────────────────────────────────────
    draw_hour_chart(frame, data, bot_cols[0]);

    // ── Panel 4: Top words ─────────────────────────────────────────────────
    let word_lines: Vec<Line> = {
        let mut v = vec![Line::from("")];
        for (word, count) in data.messages.content.top_words.iter().take(15) {
            v.push(Line::from(vec![
                ratatui::text::Span::styled(
                    format!("  {word:<14}"),
                    Style::default().fg(Color::White),
                ),
                ratatui::text::Span::styled(
                    format!("{:>6}", fmt_num(*count)),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
        v
    };
    frame.render_widget(
        Paragraph::new(word_lines)
            .block(Block::default().borders(Borders::ALL).title(" Top Words ").border_style(Style::default().fg(Color::Cyan))),
        bot_cols[1],
    );

    // ── Panel 5: Top channels ──────────────────────────────────────────────
    let ch_lines: Vec<Line> = {
        let mut v = vec![Line::from("")];
        for (name, count) in data.messages.top_channels.iter().take(15) {
            let short = if name.chars().count() > 16 {
                format!("{}…", name.chars().take(15).collect::<String>())
            } else {
                name.clone()
            };
            v.push(Line::from(vec![
                ratatui::text::Span::styled(
                    format!("  {short:<17}"),
                    Style::default().fg(Color::White),
                ),
                ratatui::text::Span::styled(
                    format!("{:>5}", fmt_num(*count)),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
        v
    };
    frame.render_widget(
        Paragraph::new(ch_lines)
            .block(Block::default().borders(Borders::ALL).title(" Top Channels ").border_style(Style::default().fg(Color::Cyan))),
        bot_cols[2],
    );
}

fn draw_hour_chart(frame: &mut ratatui::Frame<'_>, data: &analyzer::AnalysisData, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Messages by Hour (UTC) ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if data.messages.temporal.by_hour.is_empty() {
        return;
    }

    let max_count = data.messages.temporal.by_hour.values().copied().max().unwrap_or(1).max(1);
    let chart_height = inner.height.saturating_sub(2) as u64;
    let bar_width = (inner.width / 24).max(1);

    let mut lines: Vec<Line> = Vec::new();

    // Build bars line by line (top to bottom)
    for row in (0..inner.height.saturating_sub(1)).rev() {
        let threshold = (row as u64 * max_count) / inner.height.saturating_sub(1) as u64;
        let mut spans = Vec::new();
        for hour in 0u32..24 {
            let count = data.messages.temporal.by_hour.get(&hour).copied().unwrap_or(0);
            let bar_h = (count * chart_height) / max_count;
            let fill = bar_h >= (inner.height.saturating_sub(1) - row) as u64;
            let ch = if fill { "█" } else { " " };
            let color = if fill {
                // gradient: low=DarkGray, mid=Blue, high=Cyan
                let frac = count as f32 / max_count as f32;
                if frac > 0.75 { Color::Cyan } else if frac > 0.4 { Color::Blue } else { Color::DarkGray }
            } else {
                Color::Reset
            };
            for _ in 0..bar_width {
                spans.push(ratatui::text::Span::styled(ch, Style::default().fg(color)));
            }
        }
        lines.push(Line::from(spans));
        let _ = threshold;
    }

    // Hour labels (every 6h)
    let mut label_spans = Vec::new();
    for hour in 0u32..24 {
        let label = if hour % 6 == 0 {
            format!("{hour:02}")
        } else {
            " ".repeat(bar_width as usize)
        };
        let s = format!("{label:<width$}", width = bar_width as usize);
        label_spans.push(ratatui::text::Span::styled(s, Style::default().fg(Color::DarkGray)));
    }
    lines.push(Line::from(label_spans));

    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_channels(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let channels = filtered_channels(app);

    let filter_tabs = "  1:All  2:DMs  3:Groups  4:Threads  5:Voice";
    let title = format!(" Channels [{}] ", app.current_filter.label());

    if channels.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::styled("  No channels match this filter.", Style::default().fg(Color::DarkGray)),
                Line::from(""),
                Line::styled(filter_tabs, Style::default().fg(Color::DarkGray)),
            ])
            .block(Block::default().borders(Borders::ALL).title(title).border_style(Style::default().fg(Color::Cyan))),
            area,
        );
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(4)])
        .split(area);

    // Filter tabs at top
    let mut tab_spans = Vec::new();
    for (i, (filter, label)) in [
        (ChannelFilter::All,          "1:All"),
        (ChannelFilter::Dm,           "2:DMs"),
        (ChannelFilter::GroupDm,      "3:Groups"),
        (ChannelFilter::PublicThread, "4:Threads"),
        (ChannelFilter::Voice,        "5:Voice"),
    ].iter().enumerate() {
        let active = app.current_filter == *filter;
        let style = if active {
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        tab_spans.push(ratatui::text::Span::styled(format!(" {label} "), style));
        if i < 4 { tab_spans.push(ratatui::text::Span::raw("  ")); }
    }
    frame.render_widget(
        Paragraph::new(Line::from(tab_spans))
            .block(Block::default().borders(Borders::NONE)),
        rows[0],
    );

    // Channel list
    let visible_rows = rows[1].height.saturating_sub(2) as usize;
    let page_size = visible_rows.max(1);
    let start = app
        .channel_cursor
        .saturating_sub(page_size / 2)
        .min(channels.len().saturating_sub(page_size));
    let end = (start + page_size).min(channels.len());

    let max_count = channels.iter().map(|c| c.message_count).max().unwrap_or(1).max(1);

    let mut items = Vec::new();
    for (local_idx, channel) in channels[start..end].iter().enumerate() {
        let idx = start + local_idx + 1;
        let kind_color = match channel.kind {
            ChannelKind::Dm           => Color::Green,
            ChannelKind::GroupDm      => Color::LightGreen,
            ChannelKind::PublicThread => Color::Blue,
            ChannelKind::Voice        => Color::Magenta,
            ChannelKind::Guild        => Color::Yellow,
            ChannelKind::Other        => Color::DarkGray,
        };
        // mini bar (up to 8 chars)
        let bar_len = (channel.message_count * 8 / max_count).max(if channel.message_count > 0 { 1 } else { 0 });
        let bar = format!("{}{}", "█".repeat(bar_len), "░".repeat(8usize.saturating_sub(bar_len)));

        let short_title = if channel.title.chars().count() > 34 {
            format!("{}…", channel.title.chars().take(33).collect::<String>())
        } else {
            channel.title.clone()
        };

        items.push(ListItem::new(Line::from(vec![
            ratatui::text::Span::styled(format!("{idx:>4} "), Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(format!("{:<10} ", channel.kind.label()), Style::default().fg(kind_color)),
            ratatui::text::Span::styled(format!("{short_title:<35}"), Style::default().fg(Color::White)),
            ratatui::text::Span::styled(format!("{:>6} ", fmt_num(channel.message_count as u64)), Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(bar, Style::default().fg(Color::Cyan)),
        ])));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD))
        .highlight_symbol("");

    let mut state = ListState::default();
    state.select(Some(app.channel_cursor.saturating_sub(start)));
    frame.render_stateful_widget(list, rows[1], &mut state);
}

fn draw_message_view(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let Some(channel) = &app.open_channel else {
        frame.render_widget(
            Paragraph::new("No channel selected.")
                .block(Block::default().borders(Borders::ALL).title(" Messages ")),
            area,
        );
        return;
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(4)])
        .split(area);

    // Channel info header
    let kind_color = match channel.kind {
        ChannelKind::Dm           => Color::Green,
        ChannelKind::GroupDm      => Color::LightGreen,
        ChannelKind::PublicThread => Color::Blue,
        ChannelKind::Voice        => Color::Magenta,
        ChannelKind::Guild        => Color::Yellow,
        ChannelKind::Other        => Color::DarkGray,
    };
    let info = Paragraph::new(vec![
        Line::from(vec![
            ratatui::text::Span::styled("  ", Style::default()),
            ratatui::text::Span::styled(
                channel.kind.label(),
                Style::default().fg(kind_color).add_modifier(Modifier::BOLD),
            ),
            ratatui::text::Span::raw("  "),
            ratatui::text::Span::styled(
                &channel.title,
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            ratatui::text::Span::styled("  ID: ", Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(&channel.id, Style::default().fg(Color::Gray)),
            ratatui::text::Span::styled("   Messages: ", Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(
                fmt_num(channel.message_count as u64),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
        ]),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Channel Info ")
            .border_style(Style::default().fg(kind_color)),
    );
    frame.render_widget(info, rows[0]);

    // Message list
    if app.open_message_lines.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled("  No messages found.", Style::default().fg(Color::DarkGray)))
                .block(Block::default().borders(Borders::ALL).title(" Messages ").border_style(Style::default().fg(Color::Cyan))),
            rows[1],
        );
    } else {
        let lines: Vec<Line> = app.open_message_lines.iter().map(|l| {
            // Color timestamps
            if let Some(rest) = l.strip_prefix("- [") {
                if let Some(close) = rest.find(']') {
                    let ts = &rest[..close];
                    let msg = &rest[close + 1..];
                    return Line::from(vec![
                        ratatui::text::Span::styled("  [", Style::default().fg(Color::DarkGray)),
                        ratatui::text::Span::styled(ts, Style::default().fg(Color::Blue)),
                        ratatui::text::Span::styled("]", Style::default().fg(Color::DarkGray)),
                        ratatui::text::Span::styled(msg, Style::default().fg(Color::White)),
                    ]);
                }
            }
            Line::from(l.as_str())
        }).collect();

        let scroll_indicator = format!(
            " Messages  scroll: {}/{}",
            app.open_message_scroll + 1,
            app.open_message_lines.len()
        );
        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(scroll_indicator)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .wrap(Wrap { trim: false })
            .scroll((app.open_message_scroll as u16, 0));
        frame.render_widget(paragraph, rows[1]);
    }
}

fn draw_settings(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let items = vec![
        ListItem::new(Line::from(vec![
            ratatui::text::Span::styled(" Auto-download attachments  ", Style::default().fg(Color::White)),
            ratatui::text::Span::styled(
                if app.settings.download_attachments { " ON " } else { " OFF" },
                if app.settings.download_attachments {
                    Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray).bg(Color::Black)
                },
            ),
        ])),
        ListItem::new(Line::from(vec![
            ratatui::text::Span::styled(" Preview messages per channel  ", Style::default().fg(Color::White)),
            ratatui::text::Span::styled(
                format!(" {} ", app.settings.preview_messages),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            ratatui::text::Span::styled("  ← → to adjust", Style::default().fg(Color::DarkGray)),
        ])),
        ListItem::new(Line::styled(
            " Reconfigure export / results / profile",
            Style::default().fg(Color::White),
        )),
        ListItem::new(Line::styled(" Back", Style::default().fg(Color::DarkGray))),
    ];

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Settings ")
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD))
        .highlight_symbol("");

    let mut state = ListState::default();
    state.select(Some(app.settings_cursor));
    frame.render_stateful_widget(list, area, &mut state);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let px = percent_x.clamp(10, 100);
    let py = percent_y.clamp(10, 100);

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - py) / 2),
            Constraint::Percentage(py),
            Constraint::Percentage(100 - py - ((100 - py) / 2)),
        ])
        .split(area);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - px) / 2),
            Constraint::Percentage(px),
            Constraint::Percentage(100 - px - ((100 - px) / 2)),
        ])
        .split(vertical[1]);

    horizontal[1]
}

fn fit_input_for_box(input: &str, width: usize) -> (String, usize) {
    if width == 0 {
        return (String::new(), 0);
    }
    let count = input.chars().count();
    if count <= width {
        return (input.to_owned(), count);
    }

    let start = count.saturating_sub(width);
    let display = input.chars().skip(start).collect::<String>();
    (display, width)
}

fn handle_paste(app: &mut AppState, text: &str) {
    if app.screen != Screen::Setup || app.setup.step == SetupStep::Confirm {
        return;
    }
    let sanitized = text.replace(['\r', '\n'], "");
    app.setup.input.push_str(&sanitized);
}

fn handle_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    if app.screen == Screen::Analyzing || app.screen == Screen::Downloading {
        // Prevent key events while locked in processing screens unless it's Ctrl+C
        return Ok(());
    }

    if matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q'))
        && matches!(
            app.screen,
            Screen::Home | Screen::Overview | Screen::ChannelList | Screen::Settings
        )
    {
        app.should_quit = true;
        return Ok(());
    }

    match app.screen {
        Screen::Setup => handle_setup_key(app, key)?,
        Screen::Home => handle_home_key(app, key)?,
        Screen::Overview => handle_overview_key(app, key)?,
        Screen::ChannelList => handle_channel_key(app, key)?,
        Screen::MessageView => handle_message_key(app, key),
        Screen::Settings => handle_settings_key(app, key),
        _ => {}
    }

    Ok(())
}

fn handle_setup_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            app.should_quit = true;
        }
        KeyCode::Backspace => {
            if app.setup.step == SetupStep::Confirm {
                setup_prev_step(app);
            } else {
                app.setup.input.pop();
            }
        }
        KeyCode::Left | KeyCode::Up | KeyCode::BackTab => {
            setup_prev_step(app);
        }
        KeyCode::Enter | KeyCode::Tab | KeyCode::Down | KeyCode::Right => {
            if let Err(err) = setup_submit_step(app) {
                app.setup.notice = err.to_string();
            }
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if app.setup.step != SetupStep::Confirm {
                app.setup.input.clear();
            }
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if is_printable_input(c) && app.setup.step != SetupStep::Confirm {
                app.setup.input.push(c);
            }
        }
        _ => {}
    }

    Ok(())
}

fn handle_home_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    const HOME_ITEMS: usize = 10;
    const LOCKED_INDICES: [usize; 7] = [1, 2, 3, 4, 5, 6, 7];

    let is_locked = |idx: usize| app.last_data.is_none() && LOCKED_INDICES.contains(&idx);

    match key.code {
        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => {
            app.home_cursor = app.home_cursor.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') => {
            if app.home_cursor + 1 < HOME_ITEMS {
                app.home_cursor += 1;
            }
        }
        KeyCode::Enter => {
            if is_locked(app.home_cursor) {
                app.status = "Run 'Analyze Now' first to unlock this option.".to_owned();
                app.error = None;
            } else {
                match app.home_cursor {
                    0 => start_analysis(app),
                    1 => {
                        try_load_existing_data(app);
                        app.screen = Screen::Overview;
                    }
                    2 => handle_download_attachments(app),
                    3 => open_channel_filter(app, ChannelFilter::All)?,
                    4 => open_channel_filter(app, ChannelFilter::Dm)?,
                    5 => open_channel_filter(app, ChannelFilter::GroupDm)?,
                    6 => open_channel_filter(app, ChannelFilter::PublicThread)?,
                    7 => open_channel_filter(app, ChannelFilter::Voice)?,
                    8 => app.screen = Screen::Settings,
                    9 => app.should_quit = true,
                    _ => {}
                }
            }
        }
        KeyCode::Char(c) if ('1'..='9').contains(&c) => {
            let idx = (c as u8 - b'1') as usize;
            if idx < HOME_ITEMS {
                if is_locked(idx) {
                    app.home_cursor = idx; // move cursor so user can see which item is locked
                    app.status = "Run 'Analyze Now' first to unlock this option.".to_owned();
                    app.error = None;
                } else {
                    app.home_cursor = idx;
                }
            }
        }
        _ => {}
    }

    Ok(())
}

fn handle_overview_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char('b') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::Home;
        }
        KeyCode::Char('r') => {
            let results_dir = app.config.results_path(&app.config_path, &app.id);
            app.last_data = analyzer::read_data(&results_dir)?;
            app.status = "Overview refreshed from data.json".to_owned();
            app.error = None;
        }
        _ => {}
    }
    Ok(())
}

fn handle_channel_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    let count = filtered_channels(app).len();

    if count == 0 {
        if matches!(
            key.code,
            KeyCode::Char('b') | KeyCode::Esc | KeyCode::Backspace
        ) {
            app.screen = Screen::Home;
        }
        return Ok(());
    }

    match key.code {
        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => {
            app.channel_cursor = app.channel_cursor.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') => {
            if app.channel_cursor + 1 < count {
                app.channel_cursor += 1;
            }
        }
        KeyCode::PageUp | KeyCode::Char('u') => {
            app.channel_cursor = app.channel_cursor.saturating_sub(20);
        }
        KeyCode::PageDown | KeyCode::Char('d') => {
            app.channel_cursor = (app.channel_cursor + 20).min(count - 1);
        }
        KeyCode::Enter => {
            let selected = {
                let channels = filtered_channels(app);
                channels
                    .get(app.channel_cursor)
                    .map(|channel| (*channel).clone())
            };

            if let Some(channel) = selected {
                app.open_message_lines =
                    load_message_preview(&channel, app.settings.preview_messages)?;
                app.open_channel = Some(channel);
                app.open_message_scroll = 0;
                app.screen = Screen::MessageView;
            }
        }
        KeyCode::Char('1') => switch_filter(app, ChannelFilter::All)?,
        KeyCode::Char('2') => switch_filter(app, ChannelFilter::Dm)?,
        KeyCode::Char('3') => switch_filter(app, ChannelFilter::GroupDm)?,
        KeyCode::Char('4') => switch_filter(app, ChannelFilter::PublicThread)?,
        KeyCode::Char('5') => switch_filter(app, ChannelFilter::Voice)?,
        KeyCode::Char('b') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::Home;
        }
        _ => {}
    }

    Ok(())
}

fn handle_message_key(app: &mut AppState, key: KeyEvent) {
    let max_scroll = app.open_message_lines.len().saturating_sub(1);

    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            app.open_message_scroll = app.open_message_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.open_message_scroll = (app.open_message_scroll + 1).min(max_scroll);
        }
        KeyCode::PageUp => {
            app.open_message_scroll = app.open_message_scroll.saturating_sub(15);
        }
        KeyCode::PageDown => {
            app.open_message_scroll = (app.open_message_scroll + 15).min(max_scroll);
        }
        KeyCode::Char('b') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::ChannelList;
            app.open_channel = None;
            app.open_message_lines.clear();
            app.open_message_scroll = 0;
        }
        _ => {}
    }
}

fn handle_settings_key(app: &mut AppState, key: KeyEvent) {
    const ITEMS: usize = 4;

    match key.code {
        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('k') => {
            app.settings_cursor = app.settings_cursor.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('j') => {
            if app.settings_cursor + 1 < ITEMS {
                app.settings_cursor += 1;
            }
        }
        KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('h') => {
            if app.settings_cursor == 1 {
                app.settings.preview_messages =
                    app.settings.preview_messages.saturating_sub(5).max(5);
                app.save_session();
            }
        }
        KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('l') => {
            if app.settings_cursor == 1 {
                app.settings.preview_messages = (app.settings.preview_messages + 5).min(500);
                app.save_session();
            }
        }
        KeyCode::Enter => match app.settings_cursor {
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
        },
        KeyCode::Char('b') | KeyCode::Esc | KeyCode::Backspace => app.screen = Screen::Home,
        _ => {}
    }
}

fn setup_submit_step(app: &mut AppState) -> Result<()> {
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

fn setup_prev_step(app: &mut AppState) {
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

fn switch_filter(app: &mut AppState, filter: ChannelFilter) -> Result<()> {
    app.current_filter = filter;
    app.channel_cursor = 0;
    ensure_channels_loaded(app)?;
    Ok(())
}

fn open_channel_filter(app: &mut AppState, filter: ChannelFilter) -> Result<()> {
    app.current_filter = filter;
    app.channel_cursor = 0;
    ensure_channels_loaded(app)?;
    app.screen = Screen::ChannelList;
    Ok(())
}

fn key_help(screen: Screen) -> &'static str {
    match screen {
        Screen::Setup => "type/paste value  enter: next  left/up: back  esc: quit",
        Screen::Home => "w/s/↑↓: move  enter: select  1-9: quick pick  q: quit",
        Screen::Overview => "r: refresh  b/esc: back  q: quit",
        Screen::ChannelList => "w/s/↑↓: move  u/d pgup/dn: page  1-5: filter  enter: open  b: back",
        Screen::MessageView => "↑↓/k/j: scroll  pgup/dn: page  b/esc: back",
        Screen::Settings => "w/s/↑↓: move  ←→: adjust  enter: toggle/apply  b/esc: back  q: quit",
        Screen::Analyzing | Screen::Downloading => "Please wait...",
    }
}

fn format_duration(duration: Duration) -> String {
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

fn try_load_existing_data(app: &mut AppState) {
    let results_dir = app.config.results_path(&app.config_path, &app.id);
    if let Ok(data) = analyzer::read_data(&results_dir) {
        app.last_data = data;
    }
}

fn start_analysis(app: &mut AppState) {
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

fn poll_analysis(app: &mut AppState) {
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

fn handle_download_attachments(app: &mut AppState) {
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
        }).map_err(|err| err.to_string());
        let _ = tx.send(DownloadEvent::Finished(result));
    });

    app.download_rx = Some(rx);
}

fn poll_download(app: &mut AppState) {
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

fn filtered_channels(app: &AppState) -> Vec<&MessageChannel> {
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

fn ratio(part: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 / total as f64
    }
}

fn fmt_num(n: u64) -> String {
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

fn stat_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        ratatui::text::Span::styled(
            format!("  {label:<22}"),
            Style::default().fg(Color::DarkGray),
        ),
        ratatui::text::Span::styled(value.to_owned(), Style::default().fg(Color::White)),
    ])
}

fn top_counts(map: &BTreeMap<String, u64>, limit: usize) -> Vec<(String, u64)> {
    let mut items: Vec<(String, u64)> = map.iter().map(|(k, v)| (k.clone(), *v)).collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    items.truncate(limit);
    items
}

fn is_printable_input(c: char) -> bool {
    c.is_ascii() && !c.is_ascii_control()
}
