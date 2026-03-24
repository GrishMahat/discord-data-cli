pub(crate) mod handlers;

use anyhow::{Context, Result};
use crossterm::{
    event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
    terminal::size,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::{
    app::{
        AppState, Screen, screen_disabled_reason, try_load_existing_data,
        open_support_activity, open_activity, open_channel_filter, open_gallery,
        ChannelFilter,
    },
};
use self::handlers::*;

pub(crate) fn handle_paste(app: &mut AppState, text: &str) {
    if app.screen != Screen::Setup || app.setup.step == crate::app::SetupStep::Confirm {
        return;
    }
    let sanitized = text.replace(['\r', '\n'], "");
    app.setup.input.push_str(&sanitized);
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
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(root);

    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && let Some(target) = tab_at_position(chunks[1], mouse.column, mouse.row)
    {
        navigate_to_tab(app, target)?;
        return Ok(());
    }

    match app.screen {
        Screen::Analyzing => handle_analyzing_mouse(app, mouse)?,
        Screen::Home => handle_home_mouse(app, mouse, chunks[2])?,
        Screen::SupportActivity => handle_support_activity_mouse(app, mouse, chunks[2]),
        Screen::Activity => handle_activity_mouse(app, mouse, chunks[2]),
        Screen::SupportTicketDetail => handle_support_ticket_detail_mouse(app, mouse, chunks[2]),
        Screen::ActivityDetail => handle_activity_detail_mouse(app, mouse, chunks[2]),
        Screen::ChannelList => handle_channel_mouse(app, mouse, chunks[2])?,
        Screen::MessageView => handle_message_mouse(app, mouse, chunks[2]),
        Screen::Settings => handle_settings_mouse(app, mouse, chunks[2]),
        Screen::Gallery => handle_gallery_mouse(app, mouse, chunks[2]),
        _ => {}
    }

    Ok(())
}

fn tab_at_position(area: Rect, x: u16, y: u16) -> Option<Screen> {
    if !rect_contains(area, x, y) || area.width == 0 {
        return None;
    }
    let local_x = x.saturating_sub(area.x) as usize;
    let tabs = [
        (Screen::Home, "Home"),
        (Screen::Overview, "Overview"),
        (Screen::SupportActivity, "Support"),
        (Screen::Activity, "Activity"),
        (Screen::ChannelList, "Channels"),
        (Screen::Gallery, "Gallery"),
        (Screen::Settings, "Settings"),
    ];

    let mut cursor = 1usize; // leading space in draw_tabs
    for (screen, label) in tabs {
        let tab_len = label.len() + 2; // " {label} "
        if local_x >= cursor && local_x < cursor + tab_len {
            return Some(screen);
        }
        cursor += tab_len + 2; // trailing two spaces after each tab
    }
    None
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
        Screen::SupportActivity => {
            open_support_activity(app)?;
        }
        Screen::Activity => {
            open_activity(app)?;
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

    if app.screen == Screen::Analyzing || app.screen == Screen::Downloading {
        match key.code {
            KeyCode::Char('r') | KeyCode::Char('R') => {
                app.screen = Screen::Home;
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                crate::app::cancel_analysis(app);
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
                | Screen::SupportActivity
                | Screen::Activity
                | Screen::ChannelList
                | Screen::Settings
        )
    {
        app.should_quit = true;
        return Ok(());
    }

    if matches!(
        app.screen,
        Screen::Home
            | Screen::Overview
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
        let target = cycle_tab_screen(app, app.screen, reverse);
        navigate_to_tab(app, target)?;
        return Ok(());
    }

    match app.screen {
        Screen::Setup => handle_setup_key(app, key)?,
        Screen::Home => handle_home_key(app, key)?,
        Screen::Overview => handle_overview_key(app, key)?,
        Screen::SupportActivity => handle_support_activity_key(app, key)?,
        Screen::Activity => handle_activity_key(app, key)?,
        Screen::SupportTicketDetail => handle_support_ticket_detail_key(app, key),
        Screen::ActivityDetail => handle_activity_detail_key(app, key),
        Screen::ChannelList => handle_channel_key(app, key)?,
        Screen::MessageView => handle_message_key(app, key),
        Screen::Settings => handle_settings_key(app, key),
        Screen::Gallery => handle_gallery_key(app, key),
        _ => {}
    }

    Ok(())
}

fn cycle_tab_screen(app: &AppState, current: Screen, reverse: bool) -> Screen {
    let tabs = [
        Screen::Home,
        Screen::Overview,
        Screen::SupportActivity,
        Screen::Activity,
        Screen::ChannelList,
        Screen::Gallery,
        Screen::Settings,
    ];
    let current = tab_group_screen(current);
    let current_idx = tabs.iter().position(|s| *s == current).unwrap_or(0);
    let len = tabs.len();
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
        let candidate = tabs[idx];
        if screen_disabled_reason(app, candidate).is_none() {
            return candidate;
        }
    }

    Screen::Home
}

fn tab_group_screen(screen: Screen) -> Screen {
    match screen {
        Screen::SupportTicketDetail => Screen::SupportActivity,
        Screen::ActivityDetail => Screen::Activity,
        Screen::MessageView => Screen::ChannelList,
        other => other,
    }
}

pub(crate) fn rect_contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}
