use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::app::{AppState, Screen, apply_settings_selection};
use crate::input::rect_contains;

pub(crate) fn handle_settings_key(app: &mut AppState, key: KeyEvent) {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
            app.settings_cursor = app.settings_cursor.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
            if app.settings_cursor + 1 < 4 {
                app.settings_cursor += 1;
            }
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            apply_settings_selection(app);
        }
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::Home;
        }
        _ => {}
    }
}

pub(crate) fn handle_settings_mouse(app: &mut AppState, mouse: MouseEvent, area: Rect) {
    if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) || area.height <= 2 {
        return;
    }
    if !rect_contains(area, mouse.column, mouse.row) {
        return;
    }

    let row = mouse.row.saturating_sub(area.y + 1) as usize;
    if row < 4 {
        app.settings_cursor = row;
        apply_settings_selection(app);
    }
}
