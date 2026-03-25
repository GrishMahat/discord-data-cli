use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::app::{AppState, Screen, open_selected_support_ticket, refresh_support_activity_data};
use crate::data::SupportTicketView;
use crate::input::rect_contains;

pub(crate) fn handle_support_activity_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
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
            app.screen = Screen::SupportActivity;
        }
        _ => {}
    }
}

pub(crate) fn handle_support_activity_mouse(app: &mut AppState, mouse: MouseEvent, area: Rect) {
    let tickets: &[SupportTicketView] = app.support_tickets.as_deref().unwrap_or(&[]);

    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && !tickets.is_empty()
        && rect_contains(area, mouse.column, mouse.row)
        && area.height > 2
    {
        let visible_rows = area.height.saturating_sub(2) as usize;
        let page_size = visible_rows.max(1);
        let start = app
            .support_ticket_cursor
            .saturating_sub(page_size / 2)
            .min(tickets.len().saturating_sub(page_size));
        let end = (start + page_size).min(tickets.len());
        let row = mouse.row.saturating_sub(area.y + 1) as usize;
        if row < end.saturating_sub(start) {
            app.support_ticket_cursor = start + row;
            app.support_ticket_scroll = 0;
            open_selected_support_ticket(app);
        }
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
