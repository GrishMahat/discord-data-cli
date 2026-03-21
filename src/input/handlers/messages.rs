use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::{AppState, Screen};
use crate::input::rect_contains;

pub(crate) fn handle_message_key(app: &mut AppState, key: KeyEvent) {
    let max_scroll = app.open_message_lines.len().saturating_sub(1);

    match key.code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
            app.open_message_scroll = app.open_message_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
            app.open_message_scroll = (app.open_message_scroll + 1).min(max_scroll);
        }
        KeyCode::PageUp | KeyCode::Char('u') | KeyCode::Char('U') => {
            app.open_message_scroll = app.open_message_scroll.saturating_sub(15);
        }
        KeyCode::PageDown | KeyCode::Char('d') | KeyCode::Char('D') => {
            app.open_message_scroll = (app.open_message_scroll + 15).min(max_scroll);
        }
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::ChannelList;
        }
        _ => {}
    }
}

pub(crate) fn handle_message_mouse(app: &mut AppState, mouse: MouseEvent, area: Rect) {
    if !matches!(
        mouse.kind,
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
    ) {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(4)])
        .split(area);
    let message_area = rows[1];
    if !rect_contains(message_area, mouse.column, mouse.row) {
        return;
    }

    let max_scroll = app.open_message_lines.len().saturating_sub(1);
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            app.open_message_scroll = app.open_message_scroll.saturating_sub(3);
        }
        MouseEventKind::ScrollDown => {
            app.open_message_scroll = (app.open_message_scroll + 3).min(max_scroll);
        }
        _ => {}
    }
}
