use anyhow::{bail, Context, Result};
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::{atomic::Ordering, mpsc, Arc},
    thread,
    time::{Duration, Instant, SystemTime},
};

use super::*;
use crate::{analyzer, data, downloader};

#[allow(dead_code)]
pub(crate) fn log_msg(msg: &str) {
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/discord-cli.log")
    {
        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let _ = std::io::Write::write_fmt(&mut file, format_args!("[{}] {}\n", now, msg));
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
        6 => open_channel_filter(app, ChannelFilter::Dm)?,
        7 => open_channel_filter(app, ChannelFilter::PublicThread)?,
        8 => open_channel_filter(app, ChannelFilter::All)?,
        9 => app.status = "Export features coming soon.".to_owned(),
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
        5 if !folder_available(data, "messages") => {
            Some("Gallery is disabled: messages data was not included in this export.".to_owned())
        }
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
        | Screen::Downloading => return None,
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
    let package_dir = to_absolute(PathBuf::from(package_raw))?;
    if !package_dir.is_dir() {
        bail!("Export path not found: {}", package_dir.display());
    }

    let results_raw = app.setup.results_path.trim();
    let results_dir = to_absolute(PathBuf::from(results_raw))?;
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
    app.setup.notice = String::new();
    app.setup.browse_entries = list_browse_entries(&app.setup.input);
    app.setup.browse_cursor = 0;
    app.setup.browse_focus = false;
    app.setup.path_validation = validate_path(&app.setup.input);
    app.setup.browse_scroll = 0;
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
    if app.support_tickets.is_none() {
        start_support_tickets_load(app);
    }
    app.screen = Screen::SupportActivity;
    Ok(())
}

pub(crate) fn open_activity(app: &mut AppState) -> Result<()> {
    try_load_existing_data(app);
    if app.activity_events.is_none() {
        start_activity_events_load(app);
    }
    app.screen = Screen::Activity;
    Ok(())
}

pub(crate) fn start_support_tickets_load(app: &mut AppState) {
    if app.support_activity_loading {
        return;
    }
    app.support_activity_loading = true;
    app.status = "Loading support tickets in background...".to_owned();
    let (tx, rx) = mpsc::channel();
    let package_dir = app.config.package_path(&app.config_path, &app.id);
    let aliases = app.config.source_aliases.clone();
    thread::spawn(move || {
        let result = data::load_support_tickets(&package_dir, &aliases).map_err(|e| e.to_string());
        let _ = tx.send(SupportActivityEvent::TicketsFinished(result));
    });
    app.support_activity_rx = Some(rx);
}

pub(crate) fn start_activity_events_load(app: &mut AppState) {
    if app.activity_loading {
        return;
    }
    app.activity_loading = true;
    app.status = "Loading activity logs (recent 250)...".to_owned();
    let (tx, rx) = mpsc::channel();
    let package_dir = app.config.package_path(&app.config_path, &app.id);
    let aliases = app.config.source_aliases.clone();
    thread::spawn(move || {
        let result = data::load_recent_activity_events(&package_dir, &aliases, 250)
            .map_err(|e| e.to_string());
        let _ = tx.send(SupportActivityEvent::ActivityFinished(result));
    });
    app.support_activity_rx = Some(rx);
}

pub(crate) fn poll_support_activity(app: &mut AppState) {
    if let Some(rx) = &app.support_activity_rx {
        let mut closed = false;
        loop {
            match rx.try_recv() {
                Ok(SupportActivityEvent::TicketsFinished(result)) => {
                    app.support_activity_loading = false;
                    if let Ok(tickets) = result {
                        app.support_tickets = Some(tickets);
                    }
                }
                Ok(SupportActivityEvent::ActivityFinished(result)) => {
                    app.activity_loading = false;
                    if let Ok(events) = result {
                        app.activity_events = Some(events);
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    closed = true;
                    break;
                }
            }
        }
        if closed || (!app.support_activity_loading && !app.activity_loading) {
            app.support_activity_rx = None;
        }
    }
}

pub(crate) fn refresh_support_activity_data(app: &mut AppState) -> Result<()> {
    let package_dir = app.config.package_path(&app.config_path, &app.id);
    let tickets = data::load_support_tickets(&package_dir, &app.config.source_aliases)?;
    let events = data::load_recent_activity_events(&package_dir, &app.config.source_aliases, 250)?;
    app.support_tickets = Some(tickets);
    app.activity_events = Some(events);
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
            let cats = [
                "imgs", "vids", "audios", "docs", "txts", "codes", "data", "exes", "zips",
                "unknowns",
            ];
            for cat in cats {
                let cat_dir = results_dir.join(cat);
                if cat_dir.is_dir() {
                    if let Ok(entries) = fs::read_dir(cat_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.is_file() {
                                files.push(AttachmentFile {
                                    name: path
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("unknown")
                                        .to_owned(),
                                    _path: path.clone(),
                                    size: fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
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
            Ok(GalleryEvent::Finished(Ok(files))) => {
                app.gallery_loading = false;
                app.gallery.files = files;
                app.gallery_rx = None;
            }
            Ok(GalleryEvent::Finished(Err(_))) | Err(mpsc::TryRecvError::Disconnected) => {
                app.gallery_loading = false;
                app.gallery_rx = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
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
    app.analysis_abort = Arc::new(AtomicBool::new(false));
    let abort = Arc::clone(&app.analysis_abort);
    let (tx, rx) = mpsc::channel();
    let config = app.config.clone();
    let config_path = app.config_path.clone();
    let id = app.id.clone();
    thread::spawn(move || {
        let result = analyzer::run_with_progress(&config, &config_path, &id, abort, |p| {
            let _ = tx.send(AnalysisEvent::Progress(p));
        })
        .map_err(|e| e.to_string());
        let _ = tx.send(AnalysisEvent::Finished(Box::new(result)));
    });
    app.analysis_rx = Some(rx);
}

pub(crate) fn poll_analysis(app: &mut AppState) {
    if let Some(rx) = &app.analysis_rx {
        let mut finished = false;
        loop {
            match rx.try_recv() {
                Ok(AnalysisEvent::Progress(p)) => {
                    app.analysis_progress = p.fraction;
                    app.analysis_step = p.step;
                    app.status = p.label;
                    app.analysis_current_file = p.current_file;
                    app.analysis_files_processed = p.files_processed;
                    app.analysis_total_files = p.total_files;
                }
                Ok(AnalysisEvent::Finished(res)) => {
                    app.analysis_running = false;
                    app.analysis_started_at = None;
                    if let Ok(data) = *res {
                        let links = data.messages.attachment_links.clone();
                        app.last_data = Some(data);
                        app.status = "Analysis finished.".to_owned();
                        if app.settings.download_attachments && !links.is_empty() {
                            start_download(app, links);
                        } else {
                            app.screen = Screen::Overview;
                        }
                    } else if let Err(e) = *res {
                        app.error = Some(e);
                        app.status = "Analysis failed.".to_owned();
                        app.screen = Screen::Home;
                    }
                    finished = true;
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    finished = true;
                    break;
                }
            }
        }
        if finished {
            app.analysis_rx = None;
        }
    }
}

pub(crate) fn cancel_analysis(app: &mut AppState) {
    if app.analysis_running {
        app.analysis_abort.store(true, Ordering::SeqCst);
        app.status = "Canceling...".to_owned();
    }
}

pub(crate) fn handle_download_attachments(app: &mut AppState) {
    if app.download_running {
        return;
    }
    if app.last_data.is_none() {
        try_load_existing_data(app);
    }
    if let Some(data) = &app.last_data {
        if !data.messages.attachment_links.is_empty() {
            start_download(app, data.messages.attachment_links.clone());
        } else {
            app.status = "No attachments to download.".to_owned();
        }
    }
}

fn start_download(app: &mut AppState, links: Vec<String>) {
    app.download_running = true;
    app.download_progress = 0.0;
    app.screen = Screen::Downloading;
    let (tx, rx) = mpsc::channel();
    let results_dir = app.config.results_path(&app.config_path, &app.id);
    thread::spawn(move || {
        let tx2 = tx.clone();
        let result = downloader::download_attachments(&results_dir, links, move |p| {
            let _ = tx2.send(DownloadEvent::Progress(p));
        })
        .map_err(|e| e.to_string());
        let _ = tx.send(DownloadEvent::Finished(result));
    });
    app.download_rx = Some(rx);
}

pub(crate) fn poll_download(app: &mut AppState) {
    if let Some(rx) = &app.download_rx {
        let mut finished = false;
        loop {
            match rx.try_recv() {
                Ok(DownloadEvent::Progress(p)) => {
                    app.download_progress = p.fraction;
                    app.status = p.label;
                }
                Ok(DownloadEvent::Finished(_res)) => {
                    app.download_running = false;
                    app.status = "Download complete.".to_owned();
                    app.screen = Screen::Overview;
                    finished = true;
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    finished = true;
                    break;
                }
            }
        }
        if finished {
            app.download_rx = None;
        }
    }
}

pub(crate) fn open_selected_support_ticket(app: &mut AppState) {
    if app
        .support_tickets
        .as_ref()
        .is_some_and(|t| app.support_ticket_cursor < t.len())
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
    let selected = filtered_channels(app)
        .get(app.channel_cursor)
        .map(|c| (*c).clone());
    if let Some(channel) = selected {
        app.open_message_lines =
            data::load_message_preview(&channel, app.settings.preview_messages)?;
        app.open_channel = Some(channel);
        app.open_message_scroll = 0;
        app.screen = Screen::MessageView;
    }
    Ok(())
}

pub(crate) fn filtered_channels(app: &AppState) -> Vec<&data::MessageChannel> {
    app.channel_cache
        .as_ref()
        .map(|cc| {
            cc.iter()
                .filter(|c| match app.current_filter {
                    ChannelFilter::All => true,
                    ChannelFilter::Dm => c.kind == data::ChannelKind::Dm,
                    ChannelFilter::GroupDm => c.kind == data::ChannelKind::GroupDm,
                    ChannelFilter::PublicThread => c.kind == data::ChannelKind::PublicThread,
                    ChannelFilter::Voice => c.kind == data::ChannelKind::Voice,
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn ensure_channels_loaded(app: &mut AppState) -> Result<()> {
    if app.channel_cache.is_some() {
        return Ok(());
    }
    let package_dir = app.config.package_path(&app.config_path, &app.id);
    let channels = data::load_channels(&package_dir, &app.config.source_aliases)?;
    app.channel_cache = Some(channels);
    Ok(())
}

pub(crate) fn try_load_existing_data(app: &mut AppState) {
    if app.last_data.is_some() {
        return;
    }
    let results_dir = app.config.results_path(&app.config_path, &app.id);
    if let Ok(data) = analyzer::read_data(&results_dir) {
        app.last_data = data;
    }
}

pub(crate) fn key_help(screen: Screen) -> &'static str {
    match screen {
        Screen::Setup => "Enter: Next, Esc: Quit",
        Screen::Home => "Arrows: Select, Enter: Open, Q: Quit",
        Screen::Overview => "R: Refresh, B: Back",
        Screen::ChannelList => "1-5: Filter, Enter: View, B: Back",
        _ => "B: Back, Q: Quit",
    }
}

#[allow(dead_code)]
pub(crate) fn format_duration(d: Duration) -> String {
    let s = d.as_secs();
    format!("{:02}:{:02}", s / 60, s % 60)
}

pub(crate) fn is_printable_input(c: char) -> bool {
    c.is_ascii() && !c.is_ascii_control()
}

pub(crate) fn filtered_activity_events(app: &AppState) -> Vec<data::ActivityEventPreview> {
    let mut out = app.activity_events.clone().unwrap_or_default();
    out.retain(|e| activity_event_matches_filters(e, &app.activity_filters));
    match app.activity_sort {
        ActivitySortMode::Newest => out.sort_by(|a, b| b.sort_key.cmp(&a.sort_key)),
        ActivitySortMode::Oldest => out.sort_by(|a, b| a.sort_key.cmp(&b.sort_key)),
        ActivitySortMode::EventType => out.sort_by(|a, b| a.event_type.cmp(&b.event_type)),
    }
    out
}

fn activity_event_matches_filters(e: &data::ActivityEventPreview, f: &ActivityFilters) -> bool {
    let q = f.query.to_lowercase();
    if !q.is_empty()
        && !format!("{} {}", e.summary, e.event_type)
            .to_lowercase()
            .contains(&q)
    {
        return false;
    }
    true
}

pub(crate) fn filtered_gallery_files(app: &AppState) -> Vec<AttachmentFile> {
    if let Some(cat) = &app.gallery.category_filter {
        app.gallery
            .files
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
}

pub(crate) fn folder_available(data: &analyzer::AnalysisData, key: &str) -> bool {
    data.folder_presence.get(key).copied().unwrap_or(true)
}

pub(crate) fn ratio(p: u64, t: u64) -> f64 {
    if t == 0 {
        0.0
    } else {
        p as f64 / t as f64
    }
}
pub(crate) fn fmt_num(n: u64) -> String {
    n.to_string()
}
pub(crate) fn top_counts(m: &BTreeMap<String, u64>, l: usize) -> Vec<(String, u64)> {
    let mut v: Vec<_> = m.iter().map(|(k, v)| (k.clone(), *v)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v.truncate(l);
    v
}

fn to_absolute(p: PathBuf) -> Result<PathBuf> {
    if p.is_absolute() {
        Ok(p)
    } else {
        Ok(std::env::current_dir()?.join(p))
    }
}
