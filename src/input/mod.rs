pub(crate) mod handlers;

use anyhow::{Context, Result};
use crossterm::{
    event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
    terminal::size,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use self::handlers::*;
use crate::app::{
    AppState, ChannelFilter, Screen, handle_download_attachments, home_item_disabled_reason,
    open_activity, open_channel_filter, open_gallery, open_search_screen, open_support_activity,
    screen_disabled_reason, start_analysis, try_load_existing_data,
};

pub(crate) fn handle_paste(app: &mut AppState, text: &str) {
    if app.screen == Screen::Setup && app.setup.step != crate::app::SetupStep::Confirm {
        let sanitized = text.replace(['\r', '\n'], "");
        app.setup.input.push_str(&sanitized);
        return;
    }
    if app.screen == Screen::Search && app.search.field == crate::app::SearchField::Input {
        let sanitized = text.replace(['\r', '\n'], "");
        app.search.input.push_str(&sanitized);
    }
}

pub(crate) fn handle_mouse(app: &mut AppState, mouse: MouseEvent) -> Result<()> {
    if matches!(app.screen, Screen::Setup | Screen::Downloading) {
        return Ok(());
    }

    let (width, height) = size().with_context(|| "failed to read terminal size".to_owned())?;
    if width == 0 || height == 0 {
        return Ok(());
    }

    let root = Rect::new(0, 0, width, height);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(root);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Min(10)])
        .split(chunks[1]);

    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && let Some(row) = sidebar_at_position(body[0], mouse.column, mouse.row)
    {
        app.sidebar_cursor = None;
        navigate_to_sidebar_row(app, row)?;
        return Ok(());
    }

    match app.screen {
        Screen::Analyzing => handle_analyzing_mouse(app, mouse)?,
        Screen::Home => handle_home_mouse(app, mouse, body[1])?,
        Screen::Insights => handle_insights_mouse(app, mouse),
        Screen::Compare => handle_compare_mouse(app, mouse),
        Screen::ActivityMap => handle_activity_map_mouse(app, mouse),
        Screen::SupportActivity => handle_support_activity_mouse(app, mouse, body[1]),
        Screen::Activity => handle_activity_mouse(app, mouse, body[1]),
        Screen::SupportTicketDetail => handle_support_ticket_detail_mouse(app, mouse, body[1]),
        Screen::ActivityDetail => handle_activity_detail_mouse(app, mouse, body[1]),
        Screen::ChannelList => handle_channel_mouse(app, mouse, body[1])?,
        Screen::MessageView => handle_message_mouse(app, mouse, body[1]),
        Screen::Search => handle_search_mouse(app, mouse, body[1])?,
        Screen::Settings => handle_settings_mouse(app, mouse, body[1]),
        Screen::Gallery => handle_gallery_mouse(app, mouse, body[1]),
        _ => {}
    }

    Ok(())
}

fn sidebar_at_position(area: Rect, x: u16, y: u16) -> Option<usize> {
    if !rect_contains(area, x, y) || area.height <= 2 {
        return None;
    }
    let row = mouse_row_in_list(area, y);
    if row < 14 { Some(row) } else { None }
}

fn mouse_row_in_list(area: Rect, y: u16) -> usize {
    y.saturating_sub(area.y + 1) as usize
}

fn navigate_to_sidebar_row(app: &mut AppState, row: usize) -> Result<()> {
    match row {
        0 => app.screen = Screen::Home,
        1 => {
            if let Some(reason) = home_item_disabled_reason(app, 0) {
                app.status = reason;
                app.error = None;
            } else {
                start_analysis(app);
            }
        }
        2 => navigate_to_tab(app, Screen::Overview)?,
        3 => navigate_to_tab(app, Screen::Insights)?,
        4 => navigate_to_tab(app, Screen::Compare)?,
        5 => navigate_to_tab(app, Screen::ActivityMap)?,
        6 => navigate_to_tab(app, Screen::SupportActivity)?,
        7 => navigate_to_tab(app, Screen::Activity)?,
        8 => navigate_to_tab(app, Screen::Search)?,
        9 => navigate_to_tab(app, Screen::ChannelList)?,
        10 => navigate_to_tab(app, Screen::Gallery)?,
        11 => {
            if let Some(reason) = home_item_disabled_reason(app, 4) {
                app.status = reason;
                app.error = None;
            } else {
                handle_download_attachments(app);
            }
        }
        12 => navigate_to_tab(app, Screen::Settings)?,
        13 => app.should_quit = true,
        _ => {}
    }
    Ok(())
}

fn navigate_to_tab(app: &mut AppState, target: Screen) -> Result<()> {
    if let Some(reason) = screen_disabled_reason(app, target) {
        app.status = reason;
        app.error = None;
        return Ok(());
    }

    match target {
        Screen::Home => app.screen = Screen::Home,
        Screen::Overview => {
            try_load_existing_data(app);
            app.screen = Screen::Overview;
        }
        Screen::Insights => {
            try_load_existing_data(app);
            app.insights_scroll = 0;
            app.screen = Screen::Insights;
        }
        Screen::Compare => {
            try_load_existing_data(app);
            app.compare_cursor = 0;
            app.screen = Screen::Compare;
        }
        Screen::ActivityMap => {
            try_load_existing_data(app);
            app.map_page = 0;
            app.screen = Screen::ActivityMap;
        }
        Screen::SupportActivity => {
            open_support_activity(app)?;
        }
        Screen::Activity => {
            open_activity(app)?;
        }
        Screen::Search => {
            open_search_screen(app);
        }
        Screen::ChannelList => {
            open_channel_filter(app, ChannelFilter::All)?;
        }
        Screen::Gallery => {
            open_gallery(app)?;
        }
        Screen::Settings => app.screen = Screen::Settings,
        _ => {}
    }
    Ok(())
}

pub(crate) fn handle_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    // Handle Ctrl+C globally for quitting
    if matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
        && key.modifiers.contains(KeyModifiers::CONTROL)
    {
        app.should_quit = true;
        return Ok(());
    }

    if app.screen == Screen::Analyzing {
        match key.code {
            KeyCode::Char('r') | KeyCode::Char('R') => {
                app.screen = Screen::Home;
                app.sidebar_cursor = None;
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                crate::app::cancel_analysis(app);
            }
            _ => {}
        }
        return Ok(());
    }

    if app.screen == Screen::Downloading {
        match key.code {
            KeyCode::Char('r') | KeyCode::Char('R') | KeyCode::Char('c') | KeyCode::Char('C') => {
                if matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C')) {
                    crate::app::cancel_download(app);
                }
                app.screen = Screen::Home;
                app.sidebar_cursor = None;
            }
            _ => {}
        }
        return Ok(());
    }

    if matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q'))
        && matches!(
            app.screen,
            Screen::Home
                | Screen::Overview
                | Screen::Insights
                | Screen::Compare
                | Screen::ActivityMap
                | Screen::SupportActivity
                | Screen::Activity
                | Screen::Search
                | Screen::ChannelList
                | Screen::Settings
        )
    {
        app.should_quit = true;
        return Ok(());
    }

    // "/" opens message search from any main browsing screen.
    if matches!(key.code, KeyCode::Char('/'))
        && !matches!(app.screen, Screen::Setup | Screen::Search)
        && screen_disabled_reason(app, Screen::Search)
            .is_none()
    {
        open_search_screen(app);
        return Ok(());
    }

    if matches!(
        app.screen,
        Screen::Home
            | Screen::Overview
            | Screen::Insights
            | Screen::Compare
            | Screen::ActivityMap
            | Screen::Search
            | Screen::SupportActivity
            | Screen::SupportTicketDetail
            | Screen::Activity
            | Screen::ActivityDetail
            | Screen::ChannelList
            | Screen::MessageView
            | Screen::Gallery
            | Screen::Settings
    ) && matches!(key.code, KeyCode::Enter)
        && let Some(row) = app.sidebar_cursor
    {
        app.sidebar_cursor = None;
        navigate_to_sidebar_row(app, row)?;
        return Ok(());
    }

    if matches!(
        app.screen,
        Screen::Home
            | Screen::Overview
            | Screen::Insights
            | Screen::Compare
            | Screen::ActivityMap
            | Screen::Search
            | Screen::SupportActivity
            | Screen::SupportTicketDetail
            | Screen::Activity
            | Screen::ActivityDetail
            | Screen::ChannelList
            | Screen::MessageView
            | Screen::Gallery
            | Screen::Settings
    ) && matches!(key.code, KeyCode::Tab | KeyCode::BackTab)
    {
        let reverse = key.code == KeyCode::BackTab;
        let row = cycle_sidebar_row(app, reverse);
        if is_action_row(row) {
            app.sidebar_cursor = Some(row);
            app.status = sidebar_row_hint(row).to_owned();
            app.error = None;
        } else {
            app.sidebar_cursor = None;
            navigate_to_sidebar_row(app, row)?;
        }
        return Ok(());
    }

    match app.screen {
        Screen::Setup => handle_setup_key(app, key)?,
        Screen::Home => handle_home_key(app, key)?,
        Screen::Overview => handle_overview_key(app, key)?,
        Screen::Insights => handle_insights_key(app, key),
        Screen::Compare => handle_compare_key(app, key),
        Screen::ActivityMap => handle_activity_map_key(app, key),
        Screen::SupportActivity => handle_support_activity_key(app, key)?,
        Screen::Activity => handle_activity_key(app, key)?,
        Screen::SupportTicketDetail => handle_support_ticket_detail_key(app, key),
        Screen::ActivityDetail => handle_activity_detail_key(app, key),
        Screen::ChannelList => handle_channel_key(app, key)?,
        Screen::MessageView => handle_message_key(app, key),
        Screen::Search => handle_search_key(app, key)?,
        Screen::Settings => handle_settings_key(app, key),
        Screen::Gallery => handle_gallery_key(app, key),
        _ => {}
    }

    Ok(())
}

fn cycle_sidebar_row(app: &AppState, reverse: bool) -> usize {
    // Mirrors visible sidebar rows:
    // 0 Dashboard, 1 Analyze, 2 Overview, 3 Insights, 4 Compare, 5 Activity Map,
    // 6 Support, 7 Activity, 8 Search, 9 Channels, 10 Gallery, 11 Download,
    // 12 Settings, 13 Quit
    let rows = [0usize, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];
    // Use sidebar_cursor if set (action rows don't change the screen),
    // otherwise derive from current screen.
        let current = app
            .sidebar_cursor
            .unwrap_or_else(|| sidebar_row_for_screen(app));
    let current_idx = rows.iter().position(|r| *r == current).unwrap_or(0);
    let len = rows.len();
    let next_idx = if reverse {
        (current_idx + len - 1) % len
    } else {
        (current_idx + 1) % len
    };

    for i in 0..len {
        let idx = if reverse {
            (next_idx + len - i) % len
        } else {
            (next_idx + i) % len
        };
        let candidate = rows[idx];
        let dis = sidebar_row_disabled(app, candidate);
        if dis.is_none() {
            return candidate;
        }
    }
    0
}

fn sidebar_row_for_screen(app: &AppState) -> usize {
    match app.screen {
        Screen::SupportTicketDetail | Screen::SupportActivity => {
            match app.support_activity_tab {
                crate::app::SupportActivityTab::Activity => 7,
                _ => 6,
            }
        }
        Screen::ActivityDetail | Screen::Activity => 7,
        Screen::Insights => 3,
        Screen::Compare => 4,
        Screen::ActivityMap => 5,
        Screen::Search => 8,
        Screen::MessageView | Screen::ChannelList => 9,
        Screen::Gallery => 10,
        Screen::Overview => 2,
        Screen::Settings => 12,
        _ => 0,
    }
}

fn sidebar_row_disabled(app: &AppState, row: usize) -> Option<String> {
    match row {
        0 | 13 => None,                         // Dashboard/Quit always available
        1 => home_item_disabled_reason(app, 0), // Analyze
        2 => screen_disabled_reason(app, Screen::Overview),
        3 => screen_disabled_reason(app, Screen::Insights),
        4 => screen_disabled_reason(app, Screen::Compare),
        5 => screen_disabled_reason(app, Screen::ActivityMap),
        6 => screen_disabled_reason(app, Screen::SupportActivity),
        7 => screen_disabled_reason(app, Screen::Activity),
        8 => screen_disabled_reason(app, Screen::Search),
        9 => screen_disabled_reason(app, Screen::ChannelList),
        10 => screen_disabled_reason(app, Screen::Gallery),
        11 => home_item_disabled_reason(app, 4), // Download
        12 => screen_disabled_reason(app, Screen::Settings),
        _ => Some("Unknown sidebar row".to_owned()),
    }
}

fn is_action_row(row: usize) -> bool {
    matches!(row, 1 | 11 | 13)
}

fn sidebar_row_hint(row: usize) -> &'static str {
    match row {
        1 => "Analyze Now selected. Press Enter to run analysis.",
        11 => "Download selected. Press Enter to download attachments.",
        13 => "Quit selected. Press Enter to quit.",
        _ => "Press Enter to open.",
    }
}

pub(crate) fn rect_contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}
