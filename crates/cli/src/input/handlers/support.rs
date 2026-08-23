use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::{
    AppState, Screen, SupportActivityTab, open_selected_support_ticket,
    refresh_support_activity_data,
};
use crate::input::handlers::activity::handle_activity_key;
use crate::input::rect_contains;
use discord_data_cli_core::data::SupportTicketView;

pub(crate) fn handle_support_activity_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    if app.activity_filter_edit.is_none() {
        match key.code {
            KeyCode::Char('1') => {
                app.support_activity_tab = SupportActivityTab::Support;
                return Ok(());
            }
            KeyCode::Char('2') => {
                app.support_activity_tab = SupportActivityTab::Activity;
                crate::app::ensure_activity_events_loaded(app);
                return Ok(());
            }
            KeyCode::Char('3') => {
                app.support_activity_tab = SupportActivityTab::Search;
                return Ok(());
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => {
                app.support_activity_tab = app.support_activity_tab.prev();
                return Ok(());
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => {
                let was_activity = app.support_activity_tab == SupportActivityTab::Activity;
                app.support_activity_tab = app.support_activity_tab.next();
                if app.support_activity_tab == SupportActivityTab::Activity && !was_activity {
                    crate::app::ensure_activity_events_loaded(app);
                }
                return Ok(());
            }
            _ => {}
        }
    }

    match app.support_activity_tab {
        SupportActivityTab::Support => handle_support_tab_key(app, key),
        SupportActivityTab::Activity => handle_activity_key(app, key),
        SupportActivityTab::Search => handle_support_search_key(app, key),
    }
}

fn handle_support_tab_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    let ticket_count = app.support_tickets.as_ref().map(|v| v.len()).unwrap_or(0);

    match key.code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
            app.support_ticket_cursor = app.support_ticket_cursor.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
            if app.support_ticket_cursor + 1 < ticket_count {
                app.support_ticket_cursor += 1;
            }
        }
        KeyCode::Enter => open_selected_support_ticket(app),
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::Home;
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            refresh_support_activity_data(app)?;
        }
        _ => {}
    }
    Ok(())
}

fn handle_support_search_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Enter | KeyCode::Char('s') | KeyCode::Char('S') => {
            crate::app::open_search_screen(app);
        }
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::Home;
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            refresh_support_activity_data(app)?;
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn handle_support_ticket_detail_key(app: &mut AppState, key: KeyEvent) {
    let max_scroll = app
        .support_tickets
        .as_ref()
        .and_then(|tickets| tickets.get(app.support_ticket_cursor))
        .map(|ticket| ticket.detail_lines.len().saturating_sub(1))
        .unwrap_or(0);

    match key.code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
            app.support_ticket_scroll = app.support_ticket_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
            app.support_ticket_scroll = (app.support_ticket_scroll + 1).min(max_scroll);
        }
        KeyCode::PageUp => {
            app.support_ticket_scroll = app.support_ticket_scroll.saturating_sub(15);
        }
        KeyCode::PageDown => {
            app.support_ticket_scroll = (app.support_ticket_scroll + 15).min(max_scroll);
        }
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.support_activity_tab = crate::app::SupportActivityTab::Support;
            app.screen = Screen::SupportActivity;
        }
        _ => {}
    }
}

pub(crate) fn handle_support_activity_mouse(app: &mut AppState, mouse: MouseEvent, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);
    let content = rows[1];

    match app.support_activity_tab {
        SupportActivityTab::Support => {
            let tickets: &[SupportTicketView] = app.support_tickets.as_deref().unwrap_or(&[]);
            if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
                || tickets.is_empty()
                || !rect_contains(content, mouse.column, mouse.row)
                || content.height <= 2
            {
                return;
            }
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
                .split(content);
            let list_area = cols[0];
            if !rect_contains(list_area, mouse.column, mouse.row) {
                return;
            }
            let visible_rows = list_area.height.saturating_sub(2) as usize;
            let page_size = visible_rows.max(1);
            let start = app
                .support_ticket_cursor
                .saturating_sub(page_size / 2)
                .min(tickets.len().saturating_sub(page_size));
            let end = (start + page_size).min(tickets.len());
            let row = mouse.row.saturating_sub(list_area.y + 1) as usize;
            if row < end.saturating_sub(start) {
                app.support_ticket_cursor = start + row;
                app.support_ticket_scroll = 0;
                open_selected_support_ticket(app);
            }
        }
        SupportActivityTab::Activity => {
            if !matches!(
                mouse.kind,
                MouseEventKind::Down(MouseButton::Left)
                    | MouseEventKind::ScrollUp
                    | MouseEventKind::ScrollDown
            ) {
                return;
            }
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                .split(content);
            let left_rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(3)])
                .split(cols[0]);
            let list_area = left_rows[1];
            if !rect_contains(list_area, mouse.column, mouse.row) {
                return;
            }
            let filtered = crate::app::filtered_activity_events(app);
            if filtered.is_empty() || list_area.height <= 2 {
                return;
            }
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    let visible = list_area.height.saturating_sub(2) as usize;
                    let page_size = visible.max(1);
                    let cursor = app.activity_cursor.min(filtered.len().saturating_sub(1));
                    let start = cursor
                        .saturating_sub(page_size / 2)
                        .min(filtered.len().saturating_sub(page_size));
                    let end = (start + page_size).min(filtered.len());
                    let row = mouse.row.saturating_sub(list_area.y + 1) as usize;
                    if row < end.saturating_sub(start) {
                        app.activity_cursor = start + row;
                    }
                }
                MouseEventKind::ScrollUp => {
                    app.activity_cursor = app.activity_cursor.saturating_sub(1);
                }
                MouseEventKind::ScrollDown => {
                    let max_idx = filtered.len().saturating_sub(1);
                    app.activity_cursor = (app.activity_cursor + 1).min(max_idx);
                }
                _ => {}
            }
        }
        SupportActivityTab::Search => {}
    }
}

pub(crate) fn handle_support_ticket_detail_mouse(
    app: &mut AppState,
    mouse: MouseEvent,
    area: Rect,
) {
    if !rect_contains(area, mouse.column, mouse.row) {
        return;
    }

    let max_scroll = app
        .support_tickets
        .as_ref()
        .and_then(|tickets| tickets.get(app.support_ticket_cursor))
        .map(|ticket| ticket.detail_lines.len().saturating_sub(1))
        .unwrap_or(0);
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            app.support_ticket_scroll = app.support_ticket_scroll.saturating_sub(3);
        }
        MouseEventKind::ScrollDown => {
            app.support_ticket_scroll = (app.support_ticket_scroll + 3).min(max_scroll);
        }
        _ => {}
    }
}
