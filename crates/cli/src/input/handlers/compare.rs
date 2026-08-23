use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};

use crate::app::{AppState, Screen};

pub(crate) fn handle_compare_key(app: &mut AppState, key: KeyEvent) {
    let count = app.compare_snapshots().len();
    match key.code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
            app.compare_cursor = app.compare_cursor.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
            if count > 0 && app.compare_cursor + 1 < count {
                app.compare_cursor += 1;
            }
        }
        KeyCode::PageUp => app.compare_cursor = app.compare_cursor.saturating_sub(10),
        KeyCode::PageDown => {
            if count > 0 {
                app.compare_cursor = (app.compare_cursor + 10).min(count - 1);
            }
        }
        KeyCode::Home => app.compare_cursor = 0,
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::Home;
        }
        _ => {}
    }
}

pub(crate) fn handle_compare_mouse(app: &mut AppState, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::ScrollUp => app.compare_cursor = app.compare_cursor.saturating_sub(1),
        MouseEventKind::ScrollDown => {
            let count = app.compare_snapshots().len();
            if count > 0 && app.compare_cursor + 1 < count {
                app.compare_cursor += 1;
            }
        }
        _ => {}
    }
}
