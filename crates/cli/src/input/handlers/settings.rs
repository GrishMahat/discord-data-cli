use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::app::{AppState, Screen, apply_settings_selection};
use crate::ui::screens::settings::field_at;

pub(crate) fn handle_settings_key(app: &mut AppState, key: KeyEvent) {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
            app.settings_cursor = app.settings_cursor.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
            if app.settings_cursor + 1 < 5 {
                app.settings_cursor += 1;
            }
        }
        KeyCode::Left | KeyCode::Char('h') => adjust_setting(app, -1),
        KeyCode::Right | KeyCode::Char('l') => adjust_setting(app, 1),
        KeyCode::Enter | KeyCode::Char(' ') => apply_settings_selection(app),
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::Home;
        }
        _ => {}
    }
}

fn adjust_setting(app: &mut AppState, direction: i8) {
    match app.settings_cursor {
        3 => {
            let current = app.settings.preview_messages as i32;
            let next = (current + direction as i32 * 5).clamp(5, 500);
            if next != current {
                app.settings.preview_messages = next as usize;
                app.channel_preview_for = None;
                app.save_session();
            }
        }
        4 => {
            app.settings.download_attachments = !app.settings.download_attachments;
            app.save_session();
        }
        _ => {}
    }
}

pub(crate) fn handle_settings_mouse(app: &mut AppState, mouse: MouseEvent, area: Rect) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(field) = field_at(mouse.column, mouse.row, area) {
                app.settings_cursor = field;
                apply_settings_selection(app);
            }
        }
        MouseEventKind::ScrollUp => adjust_setting(app, 1),
        MouseEventKind::ScrollDown => adjust_setting(app, -1),
        _ => {}
    }
}
