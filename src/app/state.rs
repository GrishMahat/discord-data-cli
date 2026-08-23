use anyhow::{Context, Result, bail};
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::{Arc, atomic::Ordering, mpsc},
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
        0..=2 => open_setup_with_current_values(app),
        3 => {
            // Cycle preview length in steps of 5, wrapping at 500 back to 10.
            app.settings.preview_messages = if app.settings.preview_messages >= 500 {
                10
            } else {
                (app.settings.preview_messages + 5).min(500)
            };
            app.channel_preview_for = None; // force re-preview with new length
            app.save_session();
        }
        4 => {
            app.settings.download_attachments = !app.settings.download_attachments;
            app.save_session();
        }
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
        Screen::SupportActivity
            if !folder_available(data, "support_tickets")
                && !folder_available(data, "activity") =>
        {
            Some(
                "Support & Activity are disabled: source data was not included in this export."
                    .to_owned(),
            )
        }
        Screen::Activity if !folder_available(data, "activity") => {
            Some("Activity is disabled: source data was not included in this export.".to_owned())
        }
        Screen::ChannelList | Screen::Gallery | Screen::Search
            if !folder_available(data, "messages") =>
        {
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
    app.channel_preview_for = None;
    ensure_channels_loaded(app);
    Ok(())
}

pub(crate) fn open_channel_filter(app: &mut AppState, filter: ChannelFilter) -> Result<()> {
    app.current_filter = filter;
    app.channel_cursor = 0;
    app.channel_preview_for = None;
    ensure_channels_loaded(app);
    app.screen = Screen::ChannelList;
    Ok(())
}

pub(crate) fn open_search_screen(app: &mut AppState) {
    try_load_existing_data(app);
    ensure_channels_loaded(app);
    app.screen = Screen::Search;
}

pub(crate) fn open_support_activity(app: &mut AppState) -> Result<()> {
    try_load_existing_data(app);
    if app.support_tickets.is_none() && app.support_tickets_failed.is_none() {
        start_support_tickets_load(app);
    }
    app.support_activity_tab = crate::app::SupportActivityTab::Support;
    app.screen = Screen::SupportActivity;
    Ok(())
}

pub(crate) fn open_activity(app: &mut AppState) -> Result<()> {
    try_load_existing_data(app);
    if app.activity_events.is_none() && app.activity_failed.is_none() {
        start_activity_events_load(app);
    }
    app.support_activity_tab = crate::app::SupportActivityTab::Activity;
    app.screen = Screen::SupportActivity;
    Ok(())
}

pub(crate) fn start_support_tickets_load(app: &mut AppState) {
    if app.support_activity_loading || app.support_tickets_rx.is_some() {
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
    app.support_tickets_rx = Some(rx);
}

pub(crate) fn start_activity_events_load(app: &mut AppState) {
    if app.activity_loading || app.activity_events_rx.is_some() {
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
    app.activity_events_rx = Some(rx);
}

pub(crate) fn poll_support_activity(app: &mut AppState) {
    // Tickets worker.
    if let Some(rx) = &app.support_tickets_rx {
        match rx.try_recv() {
            Ok(SupportActivityEvent::TicketsFinished(Ok(tickets))) => {
                app.support_activity_loading = false;
                app.support_tickets_failed = None;
                app.support_tickets = Some(tickets);
                app.support_tickets_rx = None;
            }
            Ok(SupportActivityEvent::TicketsFinished(Err(e))) => {
                app.support_activity_loading = false;
                // Latch the failure so navigation doesn't respawn workers in a loop.
                if app.support_tickets_failed.is_none() {
                    app.support_tickets_failed = Some(e.clone());
                }
                app.status = format!("Support tickets failed to load: {e}");
                app.support_tickets_rx = None;
            }
            Ok(SupportActivityEvent::ActivityFinished(_)) => {} // wrong channel; ignore
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                app.support_activity_loading = false;
                app.support_tickets_rx = None;
            }
        }
    }
    // Activity events worker.
    if let Some(rx) = &app.activity_events_rx {
        match rx.try_recv() {
            Ok(SupportActivityEvent::ActivityFinished(Ok(events))) => {
                app.activity_loading = false;
                app.activity_failed = None;
                app.activity_events = Some(events);
                app.activity_events_rx = None;
            }
            Ok(SupportActivityEvent::ActivityFinished(Err(e))) => {
                app.activity_loading = false;
                if app.activity_failed.is_none() {
                    app.activity_failed = Some(e.clone());
                }
                app.status = format!("Activity failed to load: {e}");
                app.activity_events_rx = None;
            }
            Ok(SupportActivityEvent::TicketsFinished(_)) => {} // wrong channel; ignore
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                app.activity_loading = false;
                app.activity_events_rx = None;
            }
        }
    }
}

pub(crate) fn refresh_support_activity_data(app: &mut AppState) -> Result<()> {
    let package_dir = app.config.package_path(&app.config_path, &app.id);
    let tickets = data::load_support_tickets(&package_dir, &app.config.source_aliases)?;
    let events = data::load_recent_activity_events(&package_dir, &app.config.source_aliases, 250)?;
    app.support_tickets = Some(tickets);
    app.activity_events = Some(events);
    // Manual refresh clears failure latches so auto-load can resume later.
    app.support_tickets_failed = None;
    app.activity_failed = None;
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

pub(crate) fn cancel_download(app: &mut AppState) {
    if app.download_running {
        app.download_abort.store(true, Ordering::SeqCst);
        app.status = "Canceling download...".to_owned();
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
    app.download_abort.store(false, Ordering::SeqCst);
    app.screen = Screen::Downloading;
    let (tx, rx) = mpsc::channel();
    let results_dir = app.config.results_path(&app.config_path, &app.id);
    let abort = Arc::clone(&app.download_abort);
    thread::spawn(move || {
        let tx2 = tx.clone();
        let result =
            downloader::download_attachments(&results_dir, links, abort, move |p| {
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
                    if app.download_abort.load(Ordering::SeqCst) {
                        app.status = "Download canceled.".to_owned();
                    } else {
                        app.status = "Download complete.".to_owned();
                    }
                    if app.screen == Screen::Downloading {
                        app.screen = if app.download_abort.load(Ordering::SeqCst) {
                            Screen::Home
                        } else {
                            Screen::Overview
                        };
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
        open_channel_direct(app, channel)?;
    }
    Ok(())
}

pub(crate) fn open_channel_direct(app: &mut AppState, channel: data::MessageChannel) -> Result<()> {
    app.open_message_lines =
        data::load_message_preview(&channel, app.settings.preview_messages)?;
    app.open_channel = Some(channel);
    app.open_message_scroll = 0;
    app.screen = Screen::MessageView;
    Ok(())
}

/// Desired preview target for the channel browser split pane.
fn desired_preview_channel(app: &AppState) -> Option<data::MessageChannel> {
    filtered_channels(app)
        .get(app.channel_cursor)
        .map(|c| (*c).clone())
}

/// Background-load the message tail for the currently selected channel so the
/// split-pane browser can show a live preview without blocking the UI.
pub(crate) fn poll_channel_preview(app: &mut AppState) {
    let desired = desired_preview_channel(app);

    if !app.channel_preview_loading
        && let Some(channel) = &desired
        && app.channel_preview_for.as_deref() != Some(channel.dir_name.as_str())
    {
        app.channel_preview_loading = true;
        app.channel_preview_scroll = 0;
        let key = channel.dir_name.clone();
        let limit = app.settings.preview_messages.min(80);
        let worker_channel = channel.clone();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let result = data::load_message_preview(&worker_channel, limit)
                .map_err(|e| e.to_string());
            let _ = tx.send(ChannelPreviewEvent::Finished { key, result });
        });
        app.channel_preview_rx = Some(rx);
    }

    if let Some(rx) = &app.channel_preview_rx {
        let mut done = false;
        loop {
            match rx.try_recv() {
                Ok(ChannelPreviewEvent::Finished { key, result }) => {
                    app.channel_preview_loading = false;
                    // Only show if this is still the selected channel.
                    if desired.as_ref().is_some_and(|c| c.dir_name == key)
                        && let Ok(lines) = result
                    {
                        app.channel_preview_lines = lines;
                        app.channel_preview_for = Some(key);
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    app.channel_preview_loading = false;
                    done = true;
                    break;
                }
            }
        }
        if done {
            app.channel_preview_rx = None;
        }
    }
}

pub(crate) fn filtered_channels(app: &AppState) -> Vec<&data::MessageChannel> {
    let mut channels: Vec<&data::MessageChannel> = app
        .channel_cache
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
        .unwrap_or_default();

    match app.channel_sort {
        ChannelSortMode::Count => channels.sort_by(|a, b| {
            b.message_count
                .cmp(&a.message_count)
                .then_with(|| a.title.cmp(&b.title))
        }),
        ChannelSortMode::Name => channels.sort_by(|a, b| a.title.cmp(&b.title)),
        ChannelSortMode::Recent => {
            let last_date = |c: &data::MessageChannel| -> String {
                app.last_data
                    .as_ref()
                    .and_then(|d| d.channels_cache.get(&c.dir_name))
                    .and_then(|s| s.temporal.last_message_date.clone())
                    .unwrap_or_default()
            };
            channels.sort_by(|a, b| {
                let (la, lb) = (last_date(a), last_date(b));
                match (la.is_empty(), lb.is_empty()) {
                    (false, false) => lb.cmp(&la),
                    (true, false) => std::cmp::Ordering::Greater,
                    (false, true) => std::cmp::Ordering::Less,
                    _ => b.message_count.cmp(&a.message_count),
                }
            });
        }
    }
    channels
}

/// Cycle the channel browser sort order ('o' key).
pub(crate) fn cycle_channel_sort(app: &mut AppState) {
    app.channel_sort = app.channel_sort.next();
    app.channel_cursor = 0;
    let label = app.channel_sort.label();
    app.status = format!("Channels sorted by {label}");
}

pub(crate) fn ensure_channels_loaded(app: &mut AppState) {
    if app.channel_cache.is_some() || app.channel_loading {
        return;
    }
    app.channel_loading = true;
    app.status = "Loading channels...".to_owned();
    let (tx, rx) = mpsc::channel();
    let package_dir = app.config.package_path(&app.config_path, &app.id);
    let aliases = app.config.source_aliases.clone();
    let cached_counts: BTreeMap<String, (u64, String, String)> = app
        .last_data
        .as_ref()
        .map(|data| {
            data.channels_cache
                .iter()
                .map(|(id, c)| {
                    (id.clone(), (c.message_count, c.channel_title.clone(), c.channel_type.clone()))
                })
                .collect()
        })
        .unwrap_or_default();
    thread::spawn(move || {
        let result = data::load_channels(&package_dir, &aliases, &cached_counts)
            .map_err(|e| e.to_string());
        let _ = tx.send(ChannelEvent::Finished(result));
    });
    app.channel_rx = Some(rx);
}

pub(crate) fn poll_channels(app: &mut AppState) {
    if let Some(rx) = &app.channel_rx {
        match rx.try_recv() {
            Ok(ChannelEvent::Finished(Ok(channels))) => {
                app.channel_loading = false;
                app.channel_cache = Some(channels);
                app.channel_rx = None;
            }
            Ok(ChannelEvent::Finished(Err(e))) => {
                app.channel_loading = false;
                app.status = format!("Failed to load channels: {}", e);
                app.channel_rx = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                app.channel_loading = false;
                app.channel_rx = None;
            }
        }
    }
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

const SEARCH_MAX_RESULTS: usize = 400;
const SEARCH_BATCH_SIZE: usize = 25;

/// Kick off (or restart) a background message search over the channel cache.
pub(crate) fn start_message_search(app: &mut AppState) {
    let query = app.search.input.trim().to_owned();
    // Cancel any in-flight scan.
    app.search.cancel.store(true, Ordering::SeqCst);

    if query.is_empty() || app.search.type_filter == ChannelFilter::Voice {
        app.search.results.clear();
        app.search.running = false;
        app.search.total_matches = 0;
        app.search.scanned_files = 0;
        app.search.total_files = 0;
        return;
    }

    let channels: Vec<data::MessageChannel> = match &app.channel_cache {
        Some(cache) => cache
            .iter()
            .filter(|c| match app.search.type_filter {
                ChannelFilter::All => true,
                ChannelFilter::Dm => c.kind == data::ChannelKind::Dm,
                ChannelFilter::GroupDm => c.kind == data::ChannelKind::GroupDm,
                ChannelFilter::PublicThread => c.kind == data::ChannelKind::PublicThread,
                ChannelFilter::Voice => c.kind == data::ChannelKind::Voice,
            })
            .cloned()
            .collect(),
        None => Vec::new(),
    };

    if app.channel_cache.is_none() {
        ensure_channels_loaded(app);
        app.status = "Channels still loading — try again in a moment.".to_owned();
        return;
    }

    app.search.generation += 1;
    let generation = app.search.generation;
    app.search.results.clear();
    app.search.cursor = 0;
    app.search.scroll = 0;
    app.search.total_matches = 0;
    app.search.scanned_files = 0;
    app.search.truncated = false;
    app.search.running = true;
    app.search.cancel = Arc::new(AtomicBool::new(false));

    let has_filter = app.search.has_filter;
    let cancel = Arc::clone(&app.search.cancel);
    let total_files = channels.len();
    app.search.total_files = total_files;

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        use crate::data::utils::{
            extract_attachment_urls, extract_message_content, pick_str, stream_records,
        };

        let needle = query.to_lowercase();
        let mut total_count = 0usize;
        let mut batch: Vec<SearchResult> = Vec::new();
        let mut truncated = false;

        for (i, ch) in channels.iter().enumerate() {
            if cancel.load(Ordering::SeqCst) {
                return;
            }
            let _ = stream_records(&ch.messages_path, &mut |record| {
                if truncated || total_count + batch.len() >= SEARCH_MAX_RESULTS {
                    truncated = true;
                    return;
                }
                let content = extract_message_content(record);
                if !content.to_lowercase().contains(&needle) {
                    return;
                }
                match has_filter {
                    SearchHasFilter::Attachments => {
                        if extract_attachment_urls(record).is_empty() {
                            return;
                        }
                    }
                    SearchHasFilter::Links => {
                        if !content.contains("http") {
                            return;
                        }
                    }
                    SearchHasFilter::Any => {}
                }
                let timestamp = pick_str(
                    record,
                    &["Timestamp", "timestamp", "timestamp_ms", "date"],
                )
                .unwrap_or("unknown")
                .to_owned();
                batch.push(SearchResult {
                    channel_key: ch.dir_name.clone(),
                    title: ch.title.clone(),
                    kind: ch.kind,
                    timestamp,
                    content,
                });
            });
            if batch.len() >= SEARCH_BATCH_SIZE {
                let taken = std::mem::take(&mut batch);
                total_count += taken.len();
                let _ = tx.send(SearchEvent::Batch {
                    generation,
                    matches: taken,
                });
            }
            if i % 25 == 24 {
                let _ = tx.send(SearchEvent::Progress {
                    generation,
                    scanned_files: i + 1,
                    total_files,
                    total_matches: total_count + batch.len(),
                });
                if truncated {
                    break;
                }
            }
        }

        if !batch.is_empty() {
            total_count += batch.len();
            let _ = tx.send(SearchEvent::Batch {
                generation,
                matches: batch,
            });
        }
        let _ = tx.send(SearchEvent::Finished {
            generation,
            total_matches: total_count,
            truncated,
        });
    });
    app.search.rx = Some(rx);
}

pub(crate) fn poll_search(app: &mut AppState) {
    let current_gen = app.search.generation;
    if let Some(rx) = &app.search.rx {
        let mut finished = false;
        loop {
            match rx.try_recv() {
                Ok(SearchEvent::Batch { generation, matches }) => {
                    if generation == current_gen {
                        app.search.results.extend(matches);
                        if app.search.cursor >= app.search.results.len() {
                            app.search.cursor = app.search.results.len().saturating_sub(1);
                        }
                    }
                }
                Ok(SearchEvent::Progress {
                    generation,
                    scanned_files,
                    total_files,
                    total_matches,
                }) => {
                    if generation == current_gen {
                        app.search.scanned_files = scanned_files;
                        app.search.total_files = total_files;
                        app.search.total_matches = total_matches;
                    }
                }
                Ok(SearchEvent::Finished {
                    generation,
                    total_matches,
                    truncated,
                }) => {
                    if generation == current_gen {
                        app.search.running = false;
                        app.search.total_matches = total_matches;
                        app.search.truncated = truncated;
                        app.status = if truncated {
                            format!("Search stopped at first {} matches.", SEARCH_MAX_RESULTS)
                        } else {
                            format!("Search complete: {} matches.", total_matches)
                        };
                    }
                    finished = true;
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    app.search.running = false;
                    finished = true;
                    break;
                }
            }
        }
        if finished {
            app.search.rx = None;
        }
    }
}

/// Open the full message view for the currently selected search result.
pub(crate) fn open_search_result(app: &mut AppState) -> Result<()> {
    let Some(result) = app.search.results.get(app.search.cursor).cloned() else {
        return Ok(());
    };
    let channel = app
        .channel_cache
        .as_ref()
        .and_then(|cache| {
            cache
                .iter()
                .find(|c| c.dir_name == result.channel_key || c.id == result.channel_key)
        })
        .cloned();
    if let Some(channel) = channel {
        open_channel_direct(app, channel)?;
    } else {
        app.status = "Channel list not loaded yet — reopen search in a moment.".to_owned();
    }
    Ok(())
}

pub(crate) fn key_help(screen: Screen) -> &'static str {
    match screen {
        Screen::Setup => "Enter: Next, Esc: Quit",
        Screen::Home => "Arrows: Select, Enter: Open, Q: Quit",
        Screen::Overview => "R: Refresh, B: Back",
        Screen::Insights => "↑↓ Scroll, B: Back",
        Screen::SupportActivity => "1-3: Tabs, ↑↓: Navigate, Enter: Detail, R: Refresh, B: Back",
        Screen::SupportTicketDetail => "↑↓: Scroll, B: Back",
        Screen::ActivityDetail => "↑↓: Scroll, B: Back",
        Screen::ChannelList => "↑↓ Select, Enter Messages, 1-5 Filter, O Sort, , . Preview, B Back",
        Screen::Search => "↑↓ Section, Shift+T Query, ←→ Filter/Select, Enter Run/Open, B Back",
        Screen::Settings => "↑↓ Field, ←→ Adjust, Enter Apply, B Back",
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
    if t == 0 { 0.0 } else { p as f64 / t as f64 }
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

/// Kick off the activity-events background load if it has never succeeded
/// (and isn't already running or latched as failed).
pub(crate) fn ensure_activity_events_loaded(app: &mut AppState) {
    if app.activity_events.is_none() && app.activity_failed.is_none() {
        start_activity_events_load(app);
    }
}
